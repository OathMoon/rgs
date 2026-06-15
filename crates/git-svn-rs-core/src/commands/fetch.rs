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

pub fn run(args: FetchArgs) -> Result<(), String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: FetchArgs,
) -> Result<(), String> {
    let git = GitCli::new(work_tree.into());
    let remote = args.remote.as_deref().unwrap_or("svn");
    let config = read_remote_config(&git, remote)?;

    if config.url.starts_with("mock://") {
        let session = MockRaSession::standard_fixture("mock-uuid");
        let start_revision = next_revision(&git, &config, "mock-uuid")?;
        import_mock_revisions(
            &MockBackendFromSession(&session),
            &git,
            &config,
            ImportOptions {
                start_revision,
                end_revision: None,
            },
        )?;
        return Ok(());
    }

    let backend = SvnCliBackend::new(&config.url)?;
    let uuid = backend.uuid()?;
    let start_revision = next_revision(&git, &config, &uuid)?;
    import_mock_revisions(
        &backend,
        &git,
        &config,
        ImportOptions {
            start_revision,
            end_revision: None,
        },
    )?;
    Ok(())
}

fn read_remote_config(git: &GitCli, remote: &str) -> Result<SvnRemoteConfig, String> {
    let prefix = format!("svn-remote.{remote}");
    let url = git
        .config_get(&format!("{prefix}.url"))?
        .ok_or_else(|| format!("missing {prefix}.url"))?;
    let fetch = git.config_get_all(&format!("{prefix}.fetch"))?;
    let mappings = fetch
        .into_iter()
        .map(|value| parse_fetch_mapping(&value))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(SvnRemoteConfig {
        name: remote.to_string(),
        url,
        fetch: mappings,
        branches: Vec::new(),
        tags: Vec::new(),
        ignore_paths: git.config_get(&format!("{prefix}.ignore-paths"))?,
        include_paths: git.config_get(&format!("{prefix}.include-paths"))?,
        ignore_refs: git.config_get(&format!("{prefix}.ignore-refs"))?,
    })
}

fn parse_fetch_mapping(value: &str) -> Result<RefMapping, String> {
    let (svn_path, git_ref) = value
        .split_once(':')
        .ok_or_else(|| format!("invalid fetch mapping: {value}"))?;
    Ok(RefMapping {
        kind: MappingKind::Fetch,
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
