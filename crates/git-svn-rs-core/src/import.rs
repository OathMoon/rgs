use std::path::PathBuf;

use crate::config::SvnRemoteConfig;
use crate::fast_import::{FastImportCommit, FastImportStream, FileChange};
use crate::git::GitCli;
use crate::git_svn_id::GitSvnId;
use crate::rev_map::{ObjectFormat, RevMap};
use crate::svn::{ChangeAction, NodeKind, RevisionEvent, SvnBackend};

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
    let mapping = config
        .fetch
        .first()
        .ok_or_else(|| "svn remote has no fetch mapping".to_string())?;
    let strip_prefix = strip_prefix_for(config, &mapping.svn_path);
    let mut stream = FastImportStream::new();
    let mut imported_revisions = Vec::new();

    for (index, revision) in revisions.iter().enumerate() {
        let changes = changes_for_revision(revision, &strip_prefix);
        if changes.is_empty() {
            continue;
        }

        imported_revisions.push(revision.revision);
        stream = stream.commit(&FastImportCommit {
            mark: imported_revisions.len() as u32,
            refname: mapping.git_ref.clone(),
            author: author_ident(&revision.author),
            committer: author_ident(&revision.author),
            timestamp: index as i64,
            message: commit_message(config, revision, &uuid, &strip_prefix),
            parent_mark: (imported_revisions.len() > 1)
                .then_some(imported_revisions.len() as u32 - 1),
            changes,
        });
    }

    if imported_revisions.is_empty() {
        return Ok(ImportSummary { imported_revisions });
    }

    git.fast_import(&stream.finish())?;
    write_rev_map(git, &mapping.git_ref, &uuid, &imported_revisions)?;

    Ok(ImportSummary { imported_revisions })
}

fn changes_for_revision(revision: &RevisionEvent, strip_prefix: &str) -> Vec<FileChange> {
    if revision.changed_paths.is_empty() {
        return mock_fixture_changes(revision.revision);
    }

    revision
        .changed_paths
        .iter()
        .filter_map(|changed_path| {
            let path = import_path(&changed_path.path, strip_prefix)?;
            match changed_path.action {
                ChangeAction::Delete => Some(FileChange::Delete { path }),
                ChangeAction::Add | ChangeAction::Modify | ChangeAction::Replace
                    if changed_path.kind == NodeKind::File =>
                {
                    Some(FileChange::Modify {
                        path,
                        mode: "100644".to_string(),
                        content: changed_path.content.clone().unwrap_or_default(),
                    })
                }
                _ => None,
            }
        })
        .collect()
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
    let url = if strip_prefix.is_empty() {
        config.url.clone()
    } else {
        format!("{}/{}", config.url.trim_end_matches('/'), strip_prefix)
    };
    let footer = GitSvnId {
        url,
        revision: revision.revision,
        uuid: uuid.to_string(),
    }
    .to_footer();
    format!("{}\n\n{}", revision.message, footer)
}

fn author_ident(author: &str) -> String {
    format!("{author} <{author}@example.invalid>")
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

    let mut rev_map = RevMap::open(rev_map_path(git, refname, uuid)?, ObjectFormat::Sha1)?;
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
