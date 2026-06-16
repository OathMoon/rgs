use crate::cli::FetchArgs;
use crate::config::SvnRemoteConfig;
use crate::git::GitCli;
use crate::import::{ImportOptions, import_mock_revisions};
use crate::mapping::{MappingKind, RefMapping};
use crate::rev_map::{ObjectFormat, RevMap};
use crate::svn::SvnBackend;
use crate::svn::cli::SvnCliBackend;
use crate::svn::mock::MockRaSession;
use crate::svn::ra::RaSession;
use std::cmp;

pub fn run(args: FetchArgs) -> Result<(), String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: FetchArgs,
) -> Result<(), String> {
    let git = GitCli::new(work_tree.into());
    let remotes = if args.fetch_all {
        svn_remote_names(&git)?
    } else {
        vec![args.remote.clone().unwrap_or_else(|| "svn".to_string())]
    };

    for remote in remotes {
        fetch_remote(&git, &remote, args.shared.revision.as_deref())?;
    }
    Ok(())
}

fn fetch_remote(git: &GitCli, remote: &str, revision: Option<&str>) -> Result<(), String> {
    let config = read_remote_config(git, remote)?;

    if config.url.starts_with("mock://") {
        let session = MockRaSession::standard_fixture("mock-uuid");
        let start_revision = next_revision(git, &config, "mock-uuid")?;
        let import_options = import_options(start_revision, revision)?;
        import_mock_revisions(
            &MockBackendFromSession(&session),
            git,
            &config,
            import_options,
        )?;
        return Ok(());
    }

    let backend = SvnCliBackend::from_config(&config)?;
    let uuid = backend.uuid()?;
    let start_revision = next_revision(git, &config, &uuid)?;
    let import_options = import_options(start_revision, revision)?;
    import_mock_revisions(&backend, git, &config, import_options)?;
    Ok(())
}

fn svn_remote_names(git: &GitCli) -> Result<Vec<String>, String> {
    let keys = git.config_names_matching(r"^svn-remote\..*\.url$")?;
    let mut names = keys
        .into_iter()
        .filter_map(|key| {
            key.strip_prefix("svn-remote.")
                .and_then(|value| value.strip_suffix(".url"))
                .map(|value| value.to_string())
        })
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    Ok(names)
}

fn import_options(next_revision: u32, revision: Option<&str>) -> Result<ImportOptions, String> {
    let Some(revision) = revision else {
        return Ok(ImportOptions {
            start_revision: next_revision,
            end_revision: None,
        });
    };
    let range = parse_revision_range(revision)?;
    Ok(ImportOptions {
        start_revision: cmp::max(next_revision, range.start.unwrap_or(next_revision)),
        end_revision: range.end,
    })
}

struct RevisionRange {
    start: Option<u32>,
    end: Option<u32>,
}

fn parse_revision_range(value: &str) -> Result<RevisionRange, String> {
    if let Some((start, end)) = value.split_once(':') {
        return Ok(RevisionRange {
            start: parse_optional_revision(start)?,
            end: parse_optional_revision(end)?,
        });
    }
    let revision = parse_revision(value)?;
    Ok(RevisionRange {
        start: Some(revision),
        end: Some(revision),
    })
}

fn parse_optional_revision(value: &str) -> Result<Option<u32>, String> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    parse_revision(value).map(Some)
}

fn parse_revision(value: &str) -> Result<u32, String> {
    let trimmed = value.trim();
    let revision = trimmed.strip_prefix('r').unwrap_or(trimmed);
    revision
        .parse()
        .map_err(|_| format!("invalid SVN revision: {value}"))
}

fn read_remote_config(git: &GitCli, remote: &str) -> Result<SvnRemoteConfig, String> {
    let prefix = format!("svn-remote.{remote}");
    let url = git
        .config_get(&format!("{prefix}.url"))?
        .ok_or_else(|| format!("missing {prefix}.url"))?;
    let fetch = git.config_get_all(&format!("{prefix}.fetch"))?;
    let branches = git.config_get_all(&format!("{prefix}.branches"))?;
    let tags = git.config_get_all(&format!("{prefix}.tags"))?;
    let mappings = fetch
        .into_iter()
        .map(|value| parse_mapping(&value, MappingKind::Fetch))
        .collect::<Result<Vec<_>, _>>()?;
    let branch_mappings = branches
        .into_iter()
        .map(|value| parse_mapping(&value, MappingKind::Branches))
        .collect::<Result<Vec<_>, _>>()?;
    let tag_mappings = tags
        .into_iter()
        .map(|value| parse_mapping(&value, MappingKind::Tags))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(SvnRemoteConfig {
        name: remote.to_string(),
        url,
        fetch: mappings,
        branches: branch_mappings,
        tags: tag_mappings,
        ignore_paths: git.config_get(&format!("{prefix}.ignore-paths"))?,
        include_paths: git.config_get(&format!("{prefix}.include-paths"))?,
        ignore_refs: git.config_get(&format!("{prefix}.ignore-refs"))?,
        authors_file: git.config_get(&format!("{prefix}.authors-file"))?,
        authors_prog: git.config_get(&format!("{prefix}.authors-prog"))?,
        log_window_size: git
            .config_get(&format!("{prefix}.log-window-size"))?
            .map(|value| {
                value
                    .parse()
                    .map_err(|_| format!("invalid {prefix}.log-window-size: {value}"))
            })
            .transpose()?,
        localtime: git
            .config_get(&format!("{prefix}.localtime"))?
            .is_some_and(|value| value == "true"),
        username: git.config_get(&format!("{prefix}.username"))?,
        config_dir: git.config_get(&format!("{prefix}.config-dir"))?,
        no_auth_cache: git
            .config_get(&format!("{prefix}.no-auth-cache"))?
            .is_some_and(|value| value == "true"),
        no_metadata: git
            .config_get(&format!("{prefix}.noMetadata"))?
            .is_some_and(|value| value == "true"),
        rewrite_root: git.config_get(&format!("{prefix}.rewriteRoot"))?,
        rewrite_uuid: git.config_get(&format!("{prefix}.rewriteUUID"))?,
        preserve_empty_dirs: git
            .config_get(&format!("{prefix}.preserve-empty-dirs"))?
            .is_some_and(|value| value == "true"),
        placeholder_filename: git
            .config_get(&format!("{prefix}.placeholder-filename"))?
            .unwrap_or_else(|| ".gitignore".to_string()),
    })
}

fn parse_mapping(value: &str, kind: MappingKind) -> Result<RefMapping, String> {
    let (svn_path, git_ref) = value
        .split_once(':')
        .ok_or_else(|| format!("invalid fetch mapping: {value}"))?;
    Ok(RefMapping {
        kind,
        svn_path: svn_path.to_string(),
        git_ref: git_ref.to_string(),
    })
}

fn next_revision(git: &GitCli, config: &SvnRemoteConfig, uuid: &str) -> Result<u32, String> {
    let Some(mapping) = config.fetch.first() else {
        return Ok(1);
    };
    let git_dir = git.git_dir()?;
    let short_ref = mapping
        .git_ref
        .strip_prefix("refs/remotes/")
        .unwrap_or(&mapping.git_ref)
        .replace('/', ".");
    let rev_map_path = git
        .work_tree()
        .join(git_dir)
        .join("svn")
        .join(short_ref)
        .join(format!(".rev_map.{uuid}"));
    let rev_map = RevMap::open(rev_map_path, ObjectFormat::Sha1)?;
    Ok(rev_map.max_revision(false)?.unwrap_or(0) + 1)
}

struct MockBackendFromSession<'a>(&'a MockRaSession);

impl crate::svn::SvnBackend for MockBackendFromSession<'_> {
    fn uuid(&self) -> Result<String, String> {
        self.0.uuid()
    }

    fn latest_revnum(&self) -> Result<u32, String> {
        self.0.latest_revnum()
    }

    fn log(&self, start: u32, end: u32) -> Result<Vec<crate::svn::RevisionEvent>, String> {
        self.0.get_log(&[], start, end)
    }
}
