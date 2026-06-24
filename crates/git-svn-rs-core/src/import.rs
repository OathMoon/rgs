use std::path::PathBuf;
use std::process::Command;

use crate::authors::{AuthorResolver, parse_authors_file};
use crate::config::SvnRemoteConfig;
use crate::fast_import::{FastImportCommit, FastImportStream, FileChange};
use crate::filters::{FilterDecision, PathFilters};
use crate::git::GitCli;
use crate::git_svn_id::GitSvnId;
use crate::glob_spec::GlobSpec;
use crate::mapping::RefMapping;
use crate::rev_map::RevMap;
use crate::svn::{ChangeAction, NodeKind, RevisionEvent, SvnBackend};
use fancy_regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportOptions {
    pub start_revision: u32,
    pub end_revision: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    pub imported_revisions: Vec<u32>,
}

pub fn import_mock_revisions(
    backend: &impl SvnBackend,
    git: &GitCli,
    config: &SvnRemoteConfig,
    options: ImportOptions,
) -> Result<ImportSummary, String> {
    let end = options.end_revision.unwrap_or(backend.latest_revnum()?);
    if options.start_revision > end {
        return Ok(ImportSummary {
            imported_revisions: Vec::new(),
        });
    }

    let revisions = backend.log(options.start_revision, end)?;
    if revisions.is_empty() {
        return Ok(ImportSummary {
            imported_revisions: Vec::new(),
        });
    }

    let uuid = backend.uuid()?;
    let mappings = concrete_mappings(config, &revisions)?;
    if mappings.is_empty() {
        return Ok(ImportSummary {
            imported_revisions: Vec::new(),
        });
    }
    let mut all_imported_revisions = Vec::new();

    for mapping in &mappings {
        let summary =
            import_revisions_for_mapping(git, config, &uuid, mapping, &mappings, &revisions)?;
        all_imported_revisions.extend(summary.imported_revisions);
    }

    all_imported_revisions.sort_unstable();
    all_imported_revisions.dedup();
    Ok(ImportSummary {
        imported_revisions: all_imported_revisions,
    })
}

fn import_revisions_for_mapping(
    git: &GitCli,
    config: &SvnRemoteConfig,
    uuid: &str,
    mapping: &RefMapping,
    all_mappings: &[RefMapping],
    revisions: &[RevisionEvent],
) -> Result<ImportSummary, String> {
    let strip_prefix = strip_prefix_for(config, &mapping.svn_path);
    let mut stream = FastImportStream::new();
    let mut imported_revisions = Vec::new();
    let max_imported_revision = max_imported_revision(git, &mapping.git_ref, uuid)?;
    let existing_parent_ref = git
        .rev_parse(&mapping.git_ref)
        .ok()
        .map(|commit| commit.trim().to_string());
    let authors = author_mapper(config)?;

    for (index, revision) in revisions.iter().enumerate() {
        if revision.revision <= max_imported_revision {
            continue;
        }
        let changes = changes_for_revision(revision, &strip_prefix, config)?;
        if changes.is_empty() {
            continue;
        }

        imported_revisions.push(revision.revision);
        let first_parent_ref = if imported_revisions.len() == 1 {
            if existing_parent_ref.is_some() {
                existing_parent_ref.clone()
            } else {
                copy_from_parent_ref(git, uuid, mapping, all_mappings, revision)?
            }
        } else {
            None
        };
        stream = stream.commit(&FastImportCommit {
            mark: imported_revisions.len() as u32,
            refname: mapping.git_ref.clone(),
            author: author_ident(&revision.author, Some(&authors))?,
            committer: author_ident(&revision.author, Some(&authors))?,
            timestamp: index as i64,
            message: commit_message(config, revision, uuid, &strip_prefix),
            parent_mark: (imported_revisions.len() > 1)
                .then_some(imported_revisions.len() as u32 - 1),
            parent_ref: first_parent_ref,
            changes,
        });
    }

    if imported_revisions.is_empty() {
        return Ok(ImportSummary { imported_revisions });
    }

    git.fast_import(&stream.finish())?;
    write_rev_map(git, &mapping.git_ref, uuid, &imported_revisions)?;

    Ok(ImportSummary { imported_revisions })
}

fn copy_from_parent_ref(
    git: &GitCli,
    uuid: &str,
    mapping: &RefMapping,
    all_mappings: &[RefMapping],
    revision: &RevisionEvent,
) -> Result<Option<String>, String> {
    let mapping_path = mapping.svn_path.trim_matches('/');
    let mut copy_candidates = revision
        .changed_paths
        .iter()
        .filter(|changed_path| {
            let changed_path_text = changed_path.path.trim_matches('/');
            matches!(
                changed_path.action,
                ChangeAction::Add | ChangeAction::Replace
            ) && changed_path.kind == NodeKind::Directory
                && (changed_path_text == mapping_path
                    || changed_path_text.starts_with(&format!("{mapping_path}/")))
                && changed_path.copy_from_path.is_some()
                && changed_path.copy_from_rev.is_some()
        })
        .collect::<Vec<_>>();
    copy_candidates.sort_by_key(|changed_path| std::cmp::Reverse(changed_path.copy_from_rev));

    for copy_candidate in copy_candidates {
        let source_path = copy_candidate
            .copy_from_path
            .as_deref()
            .unwrap_or_default()
            .trim_matches('/');
        let Some(source_revision) = copy_candidate.copy_from_rev else {
            continue;
        };
        let Some(source_mapping) = all_mappings.iter().find(|candidate| {
            let candidate_path = candidate.svn_path.trim_matches('/');
            source_path == candidate_path || source_path.starts_with(&format!("{candidate_path}/"))
        }) else {
            continue;
        };

        let path = rev_map_path(git, &source_mapping.git_ref, uuid)?;
        if !path.exists() {
            continue;
        }
        if let Some(commit) = RevMap::open(path, git.object_format()?)?.get(source_revision)? {
            return Ok(Some(commit));
        }
    }

    Ok(None)
}

fn max_imported_revision(git: &GitCli, refname: &str, uuid: &str) -> Result<u32, String> {
    let rev_map = RevMap::open(rev_map_path(git, refname, uuid)?, git.object_format()?)?;
    Ok(rev_map.max_revision(false)?.unwrap_or(0))
}

fn concrete_mappings(
    config: &SvnRemoteConfig,
    revisions: &[RevisionEvent],
) -> Result<Vec<RefMapping>, String> {
    let ignore_refs = compile_ref_filter(config.ignore_refs.as_deref())?;
    let mut mappings = Vec::new();
    for mapping in &config.fetch {
        if ref_is_included(&mapping.git_ref, &ignore_refs)? {
            mappings.push(mapping.clone());
        }
    }
    for mapping in config.branches.iter().chain(config.tags.iter()) {
        let spec = GlobSpec::new(&mapping.svn_path, true)?;
        for wildcard in wildcard_matches(&spec, revisions) {
            let svn_path = spec.full_path(&wildcard);
            let git_ref = mapping.git_ref.replace('*', &wildcard);
            if !ref_is_included(&git_ref, &ignore_refs)? {
                continue;
            }
            if !mappings
                .iter()
                .any(|candidate| candidate.svn_path == svn_path && candidate.git_ref == git_ref)
            {
                mappings.push(RefMapping {
                    kind: mapping.kind.clone(),
                    svn_path,
                    git_ref,
                });
            }
        }
    }
    Ok(mappings)
}

fn compile_ref_filter(pattern: Option<&str>) -> Result<Option<Regex>, String> {
    pattern
        .map(Regex::new)
        .transpose()
        .map_err(|err| err.to_string())
}

fn ref_is_included(refname: &str, ignore_refs: &Option<Regex>) -> Result<bool, String> {
    match ignore_refs {
        Some(ignore_refs) => ignore_refs
            .is_match(refname)
            .map(|matches| !matches)
            .map_err(|err| err.to_string()),
        None => Ok(true),
    }
}

fn wildcard_matches(spec: &GlobSpec, revisions: &[RevisionEvent]) -> Vec<String> {
    let mut matches = Vec::new();
    for revision in revisions {
        for changed_path in &revision.changed_paths {
            let path = changed_path.path.trim_matches('/');
            if let Some(wildcard) = wildcard_for_path(spec, path)
                && !matches.contains(&wildcard)
            {
                matches.push(wildcard);
            }
        }
    }
    matches
}

fn wildcard_for_path(spec: &GlobSpec, path: &str) -> Option<String> {
    if spec.is_match(path) {
        return wildcard_from_exact_match(spec, path);
    }

    let left = spec.left();
    let relative = if left.is_empty() {
        path
    } else {
        path.strip_prefix(&format!("{left}/"))?
    };
    let wildcard = relative
        .split('/')
        .take(spec.depth())
        .collect::<Vec<_>>()
        .join("/");
    if wildcard.is_empty() {
        return None;
    }
    let full_path = spec.full_path(&wildcard);
    (path == full_path || path.starts_with(&format!("{full_path}/"))).then_some(wildcard)
}

fn wildcard_from_exact_match(spec: &GlobSpec, path: &str) -> Option<String> {
    let mut relative = path;
    if !spec.left().is_empty() {
        relative = relative.strip_prefix(spec.left())?.trim_start_matches('/');
    }
    if !spec.right().is_empty() {
        relative = relative.strip_suffix(spec.right())?.trim_end_matches('/');
    }
    (!relative.is_empty()).then(|| relative.to_string())
}

fn changes_for_revision(
    revision: &RevisionEvent,
    strip_prefix: &str,
    config: &SvnRemoteConfig,
) -> Result<Vec<FileChange>, String> {
    if revision.changed_paths.is_empty() {
        return Ok(mock_fixture_changes(revision.revision));
    }
    let filters = PathFilters::new(config.include_paths.clone(), config.ignore_paths.clone())?;

    let mut file_paths = Vec::new();
    for changed_path in &revision.changed_paths {
        if matches!(
            changed_path.action,
            ChangeAction::Add | ChangeAction::Modify | ChangeAction::Replace
        ) && changed_path.kind == NodeKind::File
            && path_is_included(&filters, &changed_path.path)?
            && let Some(path) = import_path(&changed_path.path, strip_prefix)
        {
            file_paths.push(path);
        }
    }

    let mut changes = Vec::new();
    for changed_path in &revision.changed_paths {
        if !path_is_included(&filters, &changed_path.path)? {
            continue;
        }
        let Some(path) = import_path(&changed_path.path, strip_prefix) else {
            continue;
        };
        match changed_path.action {
            ChangeAction::Delete
                if changed_path.kind == NodeKind::Directory && config.preserve_empty_dirs =>
            {
                changes.push(FileChange::Delete {
                    path: placeholder_path(&path, config),
                });
            }
            ChangeAction::Delete => changes.push(FileChange::Delete { path }),
            ChangeAction::Add | ChangeAction::Modify | ChangeAction::Replace
                if changed_path.kind == NodeKind::File =>
            {
                changes.push(FileChange::Modify {
                    path,
                    mode: mode_for_change(changed_path),
                    content: content_for_change(changed_path),
                });
            }
            _ => {}
        }
    }

    if config.preserve_empty_dirs {
        for changed_path in &revision.changed_paths {
            if !matches!(
                changed_path.action,
                ChangeAction::Add | ChangeAction::Replace | ChangeAction::Modify
            ) || changed_path.kind != NodeKind::Directory
            {
                continue;
            }
            if !path_is_included(&filters, &changed_path.path)? {
                continue;
            }
            let Some(path) = import_path(&changed_path.path, strip_prefix) else {
                continue;
            };
            let child_prefix = format!("{}/", path.trim_end_matches('/'));
            if file_paths
                .iter()
                .any(|file_path| file_path == &path || file_path.starts_with(&child_prefix))
            {
                continue;
            }
            changes.push(FileChange::Modify {
                path: placeholder_path(&path, config),
                mode: "100644".to_string(),
                content: Vec::new(),
            });
        }
    }

    Ok(changes)
}

fn path_is_included(filters: &PathFilters, path: &str) -> Result<bool, String> {
    let path = path.trim_matches('/');
    Ok(filters.decide(path)? == FilterDecision::Include)
}

fn placeholder_path(path: &str, config: &SvnRemoteConfig) -> String {
    format!(
        "{}/{}",
        path.trim_end_matches('/'),
        config.placeholder_filename.trim_matches('/')
    )
}

fn mode_for_change(changed_path: &crate::svn::ChangedPath) -> String {
    if changed_path.kind == NodeKind::Symlink || changed_path.properties.contains_key("svn:special")
    {
        "120000"
    } else if changed_path.properties.contains_key("svn:executable") {
        "100755"
    } else {
        "100644"
    }
    .to_string()
}

fn content_for_change(changed_path: &crate::svn::ChangedPath) -> Vec<u8> {
    let content = changed_path.content.clone().unwrap_or_default();
    if changed_path.properties.contains_key("svn:special") && content.starts_with(b"link ") {
        content[5..].to_vec()
    } else {
        content
    }
}

fn mock_fixture_changes(revision: u32) -> Vec<FileChange> {
    if revision < 2 {
        return Vec::new();
    }

    vec![FileChange::Modify {
        path: "src/lib.rs".to_string(),
        mode: "100644".to_string(),
        content: b"pub fn answer() -> u8 { 42 }\n".to_vec(),
    }]
}

fn import_path(path: &str, strip_prefix: &str) -> Option<String> {
    let path = path.trim_matches('/');
    let relative = if strip_prefix.is_empty() {
        path
    } else {
        path.strip_prefix(strip_prefix)?.trim_start_matches('/')
    };
    (!relative.is_empty()).then(|| relative.to_string())
}

fn commit_message(
    config: &SvnRemoteConfig,
    revision: &RevisionEvent,
    uuid: &str,
    strip_prefix: &str,
) -> String {
    if config.no_metadata {
        return revision.message.clone();
    }

    let root_url = config.rewrite_root.as_ref().unwrap_or(&config.url);
    let url = if strip_prefix.is_empty() {
        root_url.clone()
    } else {
        format!("{}/{}", root_url.trim_end_matches('/'), strip_prefix)
    };
    let footer = GitSvnId {
        url,
        revision: revision.revision,
        uuid: config
            .rewrite_uuid
            .clone()
            .unwrap_or_else(|| uuid.to_string()),
    }
    .to_footer();
    format!("{}\n\n{}", revision.message, footer)
}

struct AuthorMapper {
    file: Option<AuthorResolver>,
    prog: Option<String>,
}

fn author_mapper(config: &SvnRemoteConfig) -> Result<AuthorMapper, String> {
    let file = if let Some(path) = &config.authors_file {
        let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        Some(parse_authors_file(&contents)?)
    } else {
        None
    };
    Ok(AuthorMapper {
        file,
        prog: config.authors_prog.clone(),
    })
}

fn author_ident(author: &str, mapper: Option<&AuthorMapper>) -> Result<String, String> {
    if let Some(mapped) = mapper
        .and_then(|mapper| mapper.file.as_ref())
        .and_then(|resolver| resolver.resolve(author))
    {
        Ok(format!("{} <{}>", mapped.name, mapped.email))
    } else if let Some(prog) = mapper.and_then(|mapper| mapper.prog.as_ref()) {
        run_authors_prog(prog, author)
    } else {
        Ok(format!("{author} <{author}@example.invalid>"))
    }
}

fn run_authors_prog(program: &str, author: &str) -> Result<String, String> {
    let output = Command::new(program)
        .arg(author)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("authors-prog exited with status {}", output.status)
        } else {
            stderr
        });
    }
    let ident = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if ident.is_empty() || !ident.contains('<') || !ident.contains('>') {
        return Err(format!(
            "authors-prog returned invalid identity for {author}"
        ));
    }
    Ok(ident)
}

fn strip_prefix_for(config: &SvnRemoteConfig, svn_path: &str) -> String {
    if !svn_path.is_empty() {
        return svn_path.trim_matches('/').to_string();
    }

    config
        .url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split_once('/').map(|(_, path)| path))
        .unwrap_or_default()
        .trim_matches('/')
        .to_string()
}

fn write_rev_map(git: &GitCli, refname: &str, uuid: &str, revisions: &[u32]) -> Result<(), String> {
    let commits = git.run_for_test(["rev-list", "--reverse", refname])?;
    let object_ids = commits
        .lines()
        .rev()
        .take(revisions.len())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();

    if object_ids.len() != revisions.len() {
        return Err("git rev-list did not return imported commits".to_string());
    }

    let mut rev_map = RevMap::open(rev_map_path(git, refname, uuid)?, git.object_format()?)?;
    for (revision, object_id) in revisions.iter().zip(object_ids) {
        rev_map.append(*revision, object_id)?;
    }
    Ok(())
}

fn rev_map_path(git: &GitCli, refname: &str, uuid: &str) -> Result<PathBuf, String> {
    let git_dir = git.git_dir()?;
    let short_ref = refname
        .strip_prefix("refs/remotes/")
        .unwrap_or(refname)
        .replace('/', ".");
    Ok(git
        .work_tree()
        .join(git_dir)
        .join("svn")
        .join(short_ref)
        .join(format!(".rev_map.{uuid}")))
}
