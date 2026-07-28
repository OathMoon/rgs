use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(unix)]
use super::svn_fixture::SvnServe;
#[cfg(unix)]
use git_svn_rs_core::cli::DcommitArgs;
use git_svn_rs_core::cli::{
    CloneArgs, FindRevArgs, InfoArgs, LayoutArgs, LogArgs, RebaseArgs, ResetArgs, SharedFetchArgs,
};
use git_svn_rs_core::commands;
#[cfg(unix)]
use git_svn_rs_core::dcommit::journal::{BatchState, EntryState};
#[cfg(unix)]
use git_svn_rs_core::dcommit::journal_registry::discover_repository_journals;
use git_svn_rs_core::git_svn_id::GitSvnId;

const PERL_GIT_SVN_REQUIRED: &str = "Perl git-svn is required";
const FROZEN_GIT_SVN_VERSION: &str = "2.54.0";
const FROZEN_GIT_COMMIT: &str = "0b13e48a3a30cdfa94e8ef842e24d6045ab3d015";
const SVN_TOOLS_REQUIRED: &str = "svnadmin and svn are required";
const SVNSERVE_REQUIRED: &str = "svnserve is required";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompatDecision {
    Skip(String),
    Fail(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolAvailability {
    Available { version: String },
    Missing { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoldenFixtureStep {
    CreateStandardLayout,
    AddFile {
        path: &'static str,
        contents: &'static str,
    },
    SetProperty {
        path: &'static str,
        name: &'static str,
        value: &'static str,
    },
    AddEmptyDir {
        path: &'static str,
    },
    Delete {
        path: &'static str,
    },
    Copy {
        from: &'static str,
        to: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoldenFixture {
    name: &'static str,
    steps: Vec<GoldenFixtureStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevMapArtifactRecord {
    pub source_ref: String,
    pub uuid: String,
    pub revision: u32,
    pub object_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevMapByteLengthArtifact {
    pub source_ref: String,
    pub uuid: String,
    pub byte_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefTipArtifact {
    pub name: String,
    pub object_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitGraphArtifact {
    pub object_id: String,
    pub parents: Vec<String>,
    pub tree_id: String,
    pub author_name: String,
    pub author_email: String,
    pub author_epoch: i64,
    pub author_offset: String,
    pub committer_name: String,
    pub committer_email: String,
    pub committer_epoch: i64,
    pub committer_offset: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CloneStateArtifact {
    pub head_symbolic_ref: Option<String>,
    pub head_object_id: Option<String>,
    pub local_branches: Vec<String>,
    pub index_entries: Vec<String>,
    pub worktree_entries: Vec<String>,
    pub status_porcelain_v2: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustStdlayoutRefArtifact {
    pub source_ref: String,
    pub url: String,
    pub revision: u32,
    pub uuid: String,
    pub max_valid_rev_map_revision: u32,
    pub tree_contents: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileModeArtifact {
    pub mode: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePropertyArtifact {
    pub path: String,
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DcommitWriteArtifacts {
    pub svn_revision: u32,
    pub svn_uuid: String,
    pub svn_revision_metadata: String,
    pub svn_changed_paths: String,
    pub svn_tree: String,
    pub svn_contents: Vec<String>,
    pub svn_properties: Vec<String>,
    pub ref_tips: Vec<RefTipArtifact>,
    pub commit_graph: Vec<CommitGraphArtifact>,
    pub clone_state: CloneStateArtifact,
    pub git_svn_id_footers: Vec<String>,
    pub rev_map: Vec<RevMapArtifactRecord>,
    pub rev_map_byte_lengths: Vec<RevMapByteLengthArtifact>,
    pub dcommit_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GoldenComparisonArtifacts {
    pub config: Vec<(String, String)>,
    pub refs: Vec<String>,
    pub ref_tips: Vec<RefTipArtifact>,
    pub commit_graph: Vec<CommitGraphArtifact>,
    pub clone_state: CloneStateArtifact,
    pub no_checkout_clone_state: CloneStateArtifact,
    pub git_svn_id_footers: Vec<String>,
    pub rev_map: Vec<RevMapArtifactRecord>,
    pub rev_map_byte_lengths: Vec<RevMapByteLengthArtifact>,
    pub file_modes: Vec<FileModeArtifact>,
    pub file_properties: Vec<FilePropertyArtifact>,
    pub tree_contents: Vec<String>,
    pub empty_dir_placeholders: Vec<String>,
    pub log_oneline: String,
    pub log_incremental: String,
    pub log_verbose: String,
    pub log_oneline_show_commit: String,
    pub log_limit_oneline: String,
    pub log_path_oneline: String,
    pub find_rev: String,
    pub info_url: String,
    pub info_summary: String,
    pub log_revision_oneline: String,
    pub log_revision_range_oneline: String,
    pub log_revision_reverse_range_oneline: String,
    pub find_rev_nearest: String,
    pub find_rev_commit: String,
    pub rebase_dry_run: String,
    pub reset: String,
    pub gc_output: String,
    pub clone_output: String,
}

impl GoldenFixture {
    pub fn standard_linear_history() -> Self {
        Self {
            name: "standard-linear-history",
            steps: vec![
                GoldenFixtureStep::CreateStandardLayout,
                GoldenFixtureStep::AddFile {
                    path: "trunk/src/lib.rs",
                    contents: "pub fn answer() -> u8 { 42 }\n",
                },
                GoldenFixtureStep::AddFile {
                    path: "trunk/run.sh",
                    contents: "#!/bin/sh\necho hi\n",
                },
                GoldenFixtureStep::AddFile {
                    path: "trunk/link-to-lib",
                    contents: "link src/lib.rs",
                },
                GoldenFixtureStep::AddFile {
                    path: "trunk/deleted.txt",
                    contents: "temporary\n",
                },
                GoldenFixtureStep::SetProperty {
                    path: "trunk/run.sh",
                    name: "svn:executable",
                    value: "x",
                },
                GoldenFixtureStep::SetProperty {
                    path: "trunk/link-to-lib",
                    name: "svn:special",
                    value: "x",
                },
                GoldenFixtureStep::SetProperty {
                    path: "trunk/src/lib.rs",
                    name: "svn:eol-style",
                    value: "LF",
                },
                GoldenFixtureStep::SetProperty {
                    path: "trunk/src/lib.rs",
                    name: "svn:mime-type",
                    value: "text/plain",
                },
                GoldenFixtureStep::SetProperty {
                    path: "trunk/src/lib.rs",
                    name: "svn:keywords",
                    value: "Id",
                },
                GoldenFixtureStep::SetProperty {
                    path: "trunk/src/lib.rs",
                    name: "svn:needs-lock",
                    value: "x",
                },
                GoldenFixtureStep::AddEmptyDir {
                    path: "trunk/empty-dir",
                },
                GoldenFixtureStep::Copy {
                    from: "trunk",
                    to: "branches/main",
                },
                GoldenFixtureStep::Copy {
                    from: "trunk",
                    to: "tags/v1",
                },
                GoldenFixtureStep::Delete {
                    path: "trunk/deleted.txt",
                },
            ],
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn steps(&self) -> &[GoldenFixtureStep] {
        &self.steps
    }
}

#[derive(Debug, Clone)]
pub struct GoldenArtifactCapture {
    root: PathBuf,
    case_name: String,
}

impl GoldenArtifactCapture {
    pub fn new(root: impl AsRef<Path>, case_name: &str) -> Result<Self, String> {
        let root = std::env::var_os("GIT_SVN_RS_COMPAT_ARTIFACT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.as_ref().to_path_buf())
            .join(case_name);
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let capture = Self {
            root,
            case_name: case_name.to_string(),
        };
        capture.write_scenario_summary("started", None)?;
        Ok(capture)
    }

    pub fn write_text(&self, relative_path: &str, contents: &str) -> Result<PathBuf, String> {
        let path = self.root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let mut normalized = contents.replace("\r\n", "\n").replace('\r', "\n");
        if !normalized.ends_with('\n') {
            normalized.push('\n');
        }
        fs::write(&path, normalized).map_err(|e| e.to_string())?;
        Ok(path)
    }

    fn write_scenario_summary(&self, status: &str, detail: Option<&str>) -> Result<(), String> {
        let detail = detail
            .map(|value| format!(",\n  \"detail\": \"{}\"", json_escape(value)))
            .unwrap_or_default();
        let summary = format!(
            concat!(
                "{{\n",
                "  \"schema_version\": 1,\n",
                "  \"scenario\": \"{}\",\n",
                "  \"status\": \"{}\",\n",
                "  \"frozen_git_commit\": \"{}\",\n",
                "  \"git_version\": \"{}\",\n",
                "  \"git_svn_version\": \"{}\",\n",
                "  \"svn_version\": \"{}\",\n",
                "  \"rustc_version\": \"{}\",\n",
                "  \"os\": \"{}\",\n",
                "  \"artifact_profile\": \"exact-supported-subset-v1\"",
                "{}\n",
                "}}\n"
            ),
            json_escape(&self.case_name),
            json_escape(status),
            FROZEN_GIT_COMMIT,
            json_escape(&command_version("git", &["--version"])),
            json_escape(&command_version("git", &["svn", "--version"])),
            json_escape(&command_version("svn", &["--version", "--quiet"])),
            json_escape(&command_version("rustc", &["--version"])),
            std::env::consts::OS,
            detail,
        );
        fs::write(self.root.join("scenario-summary.json"), summary).map_err(|error| {
            format!(
                "failed to write scenario summary under {}: {error}",
                self.root.display()
            )
        })
    }
}

pub struct GoldenComparison {
    pub perl: GoldenComparisonArtifacts,
    pub rust: GoldenComparisonArtifacts,
    capture: GoldenArtifactCapture,
}

pub struct DcommitGoldenComparison {
    pub perl: DcommitWriteArtifacts,
    pub rust: DcommitWriteArtifacts,
    capture: GoldenArtifactCapture,
}

impl GoldenComparison {
    pub fn assert_supported_subset_matches(&self) -> Result<(), String> {
        let comparison = compare_supported_subset(&self.perl, &self.rust);
        let summary = match &comparison {
            Ok(()) => self.capture.write_scenario_summary("passed", None),
            Err(error) => self.capture.write_scenario_summary("failed", Some(error)),
        };
        comparison.and(summary)
    }
}

impl DcommitGoldenComparison {
    pub fn assert_write_artifacts_match(&self) -> Result<(), String> {
        let comparison = if self.perl == self.rust {
            Ok(())
        } else {
            Err(format!(
                "dcommit write artifacts differ\nperl: {:#?}\nrust: {:#?}",
                self.perl, self.rust
            ))
        };
        let summary = match &comparison {
            Ok(()) => self.capture.write_scenario_summary("passed", None),
            Err(error) => self.capture.write_scenario_summary("failed", Some(error)),
        };
        comparison.and(summary)
    }
}

pub fn perl_git_svn_available() -> ToolAvailability {
    let output = Command::new("git").args(["svn", "--version"]).output();
    match output {
        Ok(output) if output.status.success() => {
            classify_git_svn_version(String::from_utf8_lossy(&output.stdout).trim())
        }
        Ok(output) => ToolAvailability::Missing {
            reason: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        },
        Err(error) => ToolAvailability::Missing {
            reason: error.to_string(),
        },
    }
}

fn classify_git_svn_version(version: &str) -> ToolAvailability {
    if version.contains("git-svn-rs") {
        ToolAvailability::Missing {
            reason: format!("git svn resolved to git-svn-rs shim, not Perl git-svn: {version}"),
        }
    } else if version.split_whitespace().nth(2) != Some(FROZEN_GIT_SVN_VERSION) {
        ToolAvailability::Missing {
            reason: format!(
                "frozen Perl git-svn {FROZEN_GIT_SVN_VERSION} is required, detected: {version}"
            ),
        }
    } else {
        ToolAvailability::Available {
            version: version.to_string(),
        }
    }
}

pub fn missing_perl_git_svn_policy(strict_compat: bool) -> CompatDecision {
    if strict_compat {
        CompatDecision::Fail(PERL_GIT_SVN_REQUIRED.to_string())
    } else {
        CompatDecision::Skip(format!("skipping: {PERL_GIT_SVN_REQUIRED}"))
    }
}

pub fn require_perl_git_svn() -> Result<String, CompatDecision> {
    match perl_git_svn_available() {
        ToolAvailability::Available { version } => Ok(version),
        ToolAvailability::Missing { reason } => {
            let message = format!("{PERL_GIT_SVN_REQUIRED}: {reason}");
            if strict_compat() {
                Err(CompatDecision::Fail(message))
            } else {
                Err(CompatDecision::Skip(format!("skipping: {message}")))
            }
        }
    }
}

pub fn require_golden_tools() -> Result<String, CompatDecision> {
    let version = require_perl_git_svn()?;
    if has_svn_tools() {
        Ok(version)
    } else if strict_compat() {
        Err(CompatDecision::Fail(SVN_TOOLS_REQUIRED.to_string()))
    } else {
        Err(CompatDecision::Skip(format!(
            "skipping: {SVN_TOOLS_REQUIRED}"
        )))
    }
}

pub fn require_svn_tools() -> Result<(), CompatDecision> {
    if has_svn_tools() {
        Ok(())
    } else if strict_compat() {
        Err(CompatDecision::Fail(SVN_TOOLS_REQUIRED.to_string()))
    } else {
        Err(CompatDecision::Skip(format!(
            "skipping: {SVN_TOOLS_REQUIRED}"
        )))
    }
}

pub fn require_golden_svnserve() -> Result<(), CompatDecision> {
    if command_succeeds("svnserve", &["--version"]) {
        Ok(())
    } else if strict_compat() {
        Err(CompatDecision::Fail(SVNSERVE_REQUIRED.to_string()))
    } else {
        Err(CompatDecision::Skip(format!(
            "skipping: {SVNSERVE_REQUIRED}"
        )))
    }
}

pub fn run_rust_stdlayout_ref_artifacts(
    root: impl AsRef<Path>,
) -> Result<Vec<RustStdlayoutRefArtifact>, String> {
    let root = root.as_ref();
    let fixture = MaterializedSvnFixture::create(root)?;
    let rust_path = root.join("rust-stdlayout-clone");
    commands::clone::run(CloneArgs {
        url: fixture.url(),
        path: Some(path_arg(&rust_path)?.to_string()),
        layout: LayoutArgs {
            stdlayout: true,
            trunk: None,
            branches: Vec::new(),
            tags: Vec::new(),
            prefix: None,
        },
        shared: default_shared_fetch_args(),
        no_checkout: false,
    })?;

    let refs = run_text(
        &rust_path,
        "git",
        &["for-each-ref", "refs/remotes", "--format=%(refname)"],
    )?
    .lines()
    .map(str::to_string)
    .filter(|line| !line.ends_with("/HEAD"))
    .collect::<Vec<_>>();
    let rev_map = supported_rev_map(&rust_path, &refs)?;

    refs.into_iter()
        .map(|source_ref| {
            let message = run_text(
                &rust_path,
                "git",
                &["show", "-s", "--format=%B", &source_ref],
            )?;
            let footer = message
                .lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .ok_or_else(|| format!("missing tip message for {source_ref}"))?;
            let id = GitSvnId::parse(footer)
                .map_err(|error| format!("invalid tip git-svn-id for {source_ref}: {error}"))?;
            let max_valid_rev_map_revision = rev_map
                .iter()
                .filter(|record| record.source_ref == source_ref && record.object_id.is_some())
                .map(|record| record.revision)
                .max()
                .ok_or_else(|| format!("missing populated rev-map record for {source_ref}"))?;
            let file_modes = supported_file_modes(&rust_path, &source_ref)?;
            let tree_contents = supported_tree_contents(&rust_path, &source_ref, &file_modes)?;

            Ok(RustStdlayoutRefArtifact {
                source_ref,
                url: id.url,
                revision: id.revision,
                uuid: id.uuid,
                max_valid_rev_map_revision,
                tree_contents,
            })
        })
        .collect()
}

pub fn run_standard_trunk_golden_comparison(
    root: impl AsRef<Path>,
) -> Result<GoldenComparison, String> {
    run_standard_golden_comparison(root, GoldenLayout::Trunk)
}

pub fn run_standard_stdlayout_golden_comparison(
    root: impl AsRef<Path>,
) -> Result<GoldenComparison, String> {
    run_standard_golden_comparison(root, GoldenLayout::StdLayout)
}

pub fn run_standard_subdirectory_golden_comparison(
    root: impl AsRef<Path>,
) -> Result<GoldenComparison, String> {
    run_standard_golden_comparison(root, GoldenLayout::Subdirectory)
}

#[cfg(unix)]
pub fn run_standard_dcommit_golden_comparison(
    root: impl AsRef<Path>,
) -> Result<DcommitGoldenComparison, String> {
    run_standard_dcommit_golden_comparison_for(root, DcommitTransport::File, DcommitBehavior::Write)
}

#[cfg(unix)]
pub fn run_standard_authenticated_svn_dcommit_golden_comparison(
    root: impl AsRef<Path>,
) -> Result<DcommitGoldenComparison, String> {
    run_standard_dcommit_golden_comparison_for(
        root,
        DcommitTransport::AuthenticatedSvn,
        DcommitBehavior::Write,
    )
}

#[cfg(unix)]
pub fn run_standard_recovery_dcommit_golden_comparison(
    root: impl AsRef<Path>,
) -> Result<DcommitGoldenComparison, String> {
    run_standard_dcommit_golden_comparison_for(
        root,
        DcommitTransport::File,
        DcommitBehavior::RecoverPostFetch,
    )
}

#[cfg(unix)]
pub fn run_standard_dirty_dcommit_golden_comparison(
    root: impl AsRef<Path>,
) -> Result<DcommitGoldenComparison, String> {
    run_standard_dcommit_golden_comparison_for(
        root,
        DcommitTransport::File,
        DcommitBehavior::RejectDirty,
    )
}

#[cfg(unix)]
#[derive(Clone, Copy)]
enum DcommitTransport {
    File,
    AuthenticatedSvn,
}

#[cfg(unix)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum DcommitBehavior {
    Write,
    RecoverPostFetch,
    RejectDirty,
}

#[cfg(unix)]
fn run_standard_dcommit_golden_comparison_for(
    root: impl AsRef<Path>,
    transport: DcommitTransport,
    behavior: DcommitBehavior,
) -> Result<DcommitGoldenComparison, String> {
    use std::os::unix::fs::PermissionsExt;

    let root = root.as_ref();
    let fixture = MaterializedSvnFixture::create(root)?;
    if matches!(transport, DcommitTransport::AuthenticatedSvn) {
        configure_authenticated_svnserve(&fixture.repo)?;
    }
    let capture_name = match (transport, behavior) {
        (DcommitTransport::File, DcommitBehavior::RecoverPostFetch) => "recovered-dcommit-write",
        (DcommitTransport::File, DcommitBehavior::RejectDirty) => "dirty-dcommit-no-write",
        (DcommitTransport::File, DcommitBehavior::Write) => "standard-dcommit-write",
        (DcommitTransport::AuthenticatedSvn, _) => "authenticated-svn-dcommit-write",
    };
    let capture = GoldenArtifactCapture::new(root, capture_name)?;
    let fixed_date = "2026-07-27T12:34:56.000000Z";
    let hook = fixture.repo.join("hooks").join("post-commit");
    let date_file = fixture.repo.join("hooks").join("compat-date");
    fs::write(&date_file, fixed_date).map_err(|error| error.to_string())?;
    fs::write(
        &hook,
        "#!/bin/sh\nexec svnadmin setrevprop \"$1\" -r \"$2\" svn:date \"$1/hooks/compat-date\"\n",
    )
    .map_err(|error| error.to_string())?;
    let mut permissions = fs::metadata(&hook)
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&hook, permissions).map_err(|error| error.to_string())?;

    let standby_repo = root.join("standby-svn-repo");
    run(
        root,
        "svnadmin",
        &[
            "hotcopy",
            path_arg(&fixture.repo)?,
            path_arg(&standby_repo)?,
        ],
    )?;

    let server = match transport {
        DcommitTransport::File => None,
        DcommitTransport::AuthenticatedSvn => Some(SvnServe::start(root)?),
    };
    let fixture_url = server
        .as_ref()
        .map(|server| server.repository_url("svn-repo"))
        .unwrap_or_else(|| fixture.url());
    let perl_config_dir = root.join("perl-svn-config");
    if matches!(transport, DcommitTransport::AuthenticatedSvn) {
        prewarm_perl_svn_credentials(root, &fixture_url, &perl_config_dir)?;
    }
    let recovery_authors_prog = (behavior == DcommitBehavior::RecoverPostFetch)
        .then(|| root.join("recovery-authors-prog"))
        .map(|path| {
            write_recovery_authors_prog(&path, false)?;
            Ok::<_, String>(path)
        })
        .transpose()?;
    let perl_path = root.join("perl-dcommit");
    clone_perl_dcommit_repo(
        root,
        &fixture_url,
        &perl_path,
        transport,
        &perl_config_dir,
        recovery_authors_prog.as_deref(),
    )?;
    let rust_path = root.join("rust-dcommit");
    let mut rust_shared = dcommit_shared_fetch_args(transport);
    rust_shared.authors_prog = recovery_authors_prog
        .as_ref()
        .map(|path| path_arg(path).map(str::to_string))
        .transpose()?;
    commands::clone::run(CloneArgs {
        url: fixture_url,
        path: Some(path_arg(&rust_path)?.to_string()),
        layout: golden_layout(match transport {
            DcommitTransport::File => GoldenLayout::StdLayout,
            DcommitTransport::AuthenticatedSvn => GoldenLayout::Trunk,
        }),
        shared: rust_shared.clone(),
        no_checkout: false,
    })?;

    prepare_dcommit_commit(&perl_path)?;
    prepare_dcommit_commit(&rust_path)?;
    if behavior == DcommitBehavior::RejectDirty {
        make_dcommit_worktree_dirty(&perl_path)?;
        make_dcommit_worktree_dirty(&rust_path)?;
    }
    let perl_output = run_perl_dcommit(
        &perl_path,
        transport,
        &perl_config_dir,
        recovery_authors_prog.as_deref(),
        behavior == DcommitBehavior::RejectDirty,
    )?;
    if behavior == DcommitBehavior::RejectDirty {
        assert_repository_revision(&fixture.repo, 6, "Perl dirty preflight")?;
    }

    let perl_repo = root.join("perl-result-svn-repo");
    fs::rename(&fixture.repo, &perl_repo).map_err(|error| error.to_string())?;
    fs::rename(&standby_repo, &fixture.repo).map_err(|error| error.to_string())?;

    let dcommit_args = || DcommitArgs {
        dry_run: false,
        adopt_revision: None,
        commit_url: None,
        mergeinfo: None,
        no_rebase: true,
        shared: rust_shared.clone(),
    };
    let rust_capture = if behavior == DcommitBehavior::RejectDirty {
        let failure = commands::dcommit::run_in_work_tree(&rust_path, dcommit_args())
            .expect_err("dirty tracked worktree must fail dcommit");
        if !failure.contains("dcommit requires a clean index and working tree") {
            return Err(format!(
                "dirty dcommit did not report the clean-worktree preflight: {failure}"
            ));
        }
        assert_repository_revision(&fixture.repo, 6, "Rust dirty preflight")?;
        assert_no_dcommit_journal(&rust_path)?;
        CapturedCommandOutput {
            status: 1,
            stdout: String::new(),
            stderr: failure,
        }
    } else if let Some(authors_prog) = recovery_authors_prog.as_deref() {
        write_recovery_authors_prog(authors_prog, true)?;
        let failure = commands::dcommit::run_in_work_tree(&rust_path, dcommit_args())
            .expect_err("post-submit authors failure must interrupt dcommit");
        if !failure.contains("post-submit") {
            return Err(format!(
                "recovery dcommit did not report a post-submit failure: {failure}"
            ));
        }
        let submitted_revision =
            run_text(root, "svnlook", &["youngest", path_arg(&fixture.repo)?])?;
        if submitted_revision.trim() != "7" {
            return Err(format!(
                "failed dcommit submitted an unexpected SVN revision: {submitted_revision:?}"
            ));
        }
        let git_dir = run_text(&rust_path, "git", &["rev-parse", "--absolute-git-dir"])?;
        let discovery = discover_repository_journals(Path::new(git_dir.trim()).join("svn"))
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "post-submit failure did not persist a dcommit journal".to_string())?;
        let active = discovery
            .active
            .ok_or_else(|| "post-submit failure did not retain an active journal".to_string())?;
        if active.journal.batch_state != BatchState::Submitting
            || active.journal.entries.len() != 1
            || active.journal.entries[0].state != (EntryState::Submitted { svn_revision: 7 })
        {
            return Err(format!(
                "post-submit journal did not durably record r7: {:#?}",
                active.journal
            ));
        }
        let tracking_message = run_text(
            &rust_path,
            "git",
            &["show", "-s", "--format=%B", "refs/remotes/origin/trunk"],
        )?;
        let tracking_footer = tracking_message
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .ok_or_else(|| "tracking ref has no git-svn-id footer".to_string())?;
        let tracking_id = GitSvnId::parse(tracking_footer)
            .map_err(|error| format!("invalid tracking footer after failed fetch: {error}"))?;
        if tracking_id.revision != 6 {
            return Err(format!(
                "post-submit failure published tracking revision {} before recovery",
                tracking_id.revision
            ));
        }
        capture.write_text("rust/recovery-failure.txt", &failure)?;
        write_recovery_authors_prog(authors_prog, false)?;
        let rust_output = commands::dcommit::run_in_work_tree(&rust_path, dcommit_args())?;
        let recovered_revision =
            run_text(root, "svnlook", &["youngest", path_arg(&fixture.repo)?])?;
        if recovered_revision.trim() != "7" {
            return Err(format!(
                "recovery resubmitted the Git commit as another SVN revision: {recovered_revision:?}"
            ));
        }
        CapturedCommandOutput {
            status: 0,
            stdout: rust_output,
            stderr: String::new(),
        }
    } else {
        let rust_output = commands::dcommit::run_in_work_tree(&rust_path, dcommit_args())?;
        CapturedCommandOutput {
            status: 0,
            stdout: rust_output,
            stderr: String::new(),
        }
    };

    let expected_revision = if behavior == DcommitBehavior::RejectDirty {
        6
    } else {
        7
    };
    let perl_summary = normalize_dcommit_summary(&perl_output, expected_revision)?;
    let perl = collect_dcommit_write_artifacts(&perl_path, &perl_repo, perl_summary)?;
    let rust_summary = normalize_dcommit_summary(&rust_capture, expected_revision)?;
    let rust = collect_dcommit_write_artifacts(&rust_path, &fixture.repo, rust_summary)?;
    write_dcommit_artifacts(&capture, "perl", &perl, &perl_output)?;
    write_dcommit_artifacts(&capture, "rust", &rust, &rust_capture)?;

    Ok(DcommitGoldenComparison {
        perl,
        rust,
        capture,
    })
}

#[cfg(unix)]
fn configure_authenticated_svnserve(repository: &Path) -> Result<(), String> {
    let conf = repository.join("conf");
    fs::write(
        conf.join("svnserve.conf"),
        "[general]\nanon-access = none\nauth-access = write\npassword-db = passwd\nrealm = git-svn-rs-golden\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(conf.join("passwd"), "[users]\nalice = secret\n").map_err(|error| error.to_string())
}

#[cfg(unix)]
fn prewarm_perl_svn_credentials(root: &Path, url: &str, config_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(config_dir).map_err(|error| error.to_string())?;
    fs::write(config_dir.join("config"), "[auth]\npassword-stores =\n")
        .map_err(|error| error.to_string())?;
    fs::write(
        config_dir.join("servers"),
        "[global]\nstore-auth-creds = yes\nstore-passwords = yes\nstore-plaintext-passwords = yes\n",
    )
    .map_err(|error| error.to_string())?;
    run(
        root,
        "svn",
        &[
            "--config-dir",
            path_arg(config_dir)?,
            "info",
            "--non-interactive",
            "--username",
            "alice",
            "--password",
            "secret",
            url,
        ],
    )?;
    let simple_auth = config_dir.join("auth").join("svn.simple");
    let cached = simple_auth
        .read_dir()
        .map_err(|error| format!("failed to inspect isolated SVN auth cache: {error}"))?
        .any(|entry| entry.is_ok_and(|entry| entry.path().is_file()));
    if !cached {
        return Err(format!(
            "SVN did not persist credentials under {}",
            simple_auth.display()
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn clone_perl_dcommit_repo(
    root: &Path,
    url: &str,
    destination: &Path,
    transport: DcommitTransport,
    config_dir: &Path,
    authors_prog: Option<&Path>,
) -> Result<(), String> {
    let mut args = vec![
        "svn".to_string(),
        "clone".to_string(),
        "--prefix=origin/".to_string(),
        "--preserve-empty-dirs".to_string(),
        "--placeholder-filename".to_string(),
        ".gitkeep".to_string(),
    ];
    match transport {
        DcommitTransport::File => args.push("--stdlayout".to_string()),
        DcommitTransport::AuthenticatedSvn => {
            args.extend(["--trunk".to_string(), "trunk".to_string()]);
        }
    }
    if matches!(transport, DcommitTransport::AuthenticatedSvn) {
        args.extend([
            "--config-dir".to_string(),
            path_arg(config_dir)?.to_string(),
            "--username".to_string(),
            "alice".to_string(),
        ]);
    }
    if let Some(authors_prog) = authors_prog {
        args.extend([
            "--authors-prog".to_string(),
            path_arg(authors_prog)?.to_string(),
        ]);
    }
    args.extend([url.to_string(), path_arg(destination)?.to_string()]);
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    match transport {
        DcommitTransport::File => run(root, "git", &args),
        DcommitTransport::AuthenticatedSvn => {
            run_capture_with_stdin(root, "git", &args, "secret\nsecret\nsecret\n").map(|_| ())
        }
    }
}

#[cfg(unix)]
fn write_recovery_authors_prog(path: &Path, fail: bool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let script = if fail {
        "#!/bin/sh\nexit 1\n"
    } else {
        "#!/bin/sh\necho 'Recovery Author <recovery@example.com>'\n"
    };
    fs::write(path, script).map_err(|error| error.to_string())?;
    let mut permissions = fs::metadata(path)
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(|error| error.to_string())
}

#[cfg(unix)]
fn make_dcommit_worktree_dirty(work_tree: &Path) -> Result<(), String> {
    fs::write(
        work_tree.join("src/lib.rs"),
        "pub fn answer() -> u8 { 99 }\n",
    )
    .map_err(|error| error.to_string())?;
    run(work_tree, "git", &["add", "src/lib.rs"])
}

#[cfg(unix)]
fn assert_repository_revision(
    repository: &Path,
    expected: u32,
    context: &str,
) -> Result<(), String> {
    let actual = run_text(repository, "svnlook", &["youngest", path_arg(repository)?])?;
    if actual.trim() == expected.to_string() {
        Ok(())
    } else {
        Err(format!(
            "{context} changed SVN from r{expected} to r{}",
            actual.trim()
        ))
    }
}

#[cfg(unix)]
fn assert_no_dcommit_journal(work_tree: &Path) -> Result<(), String> {
    let git_dir = run_text(work_tree, "git", &["rev-parse", "--absolute-git-dir"])?;
    let discovery = discover_repository_journals(Path::new(git_dir.trim()).join("svn"))
        .map_err(|error| error.to_string())?;
    if discovery.is_none() {
        Ok(())
    } else {
        Err("dirty preflight created a dcommit journal before rejecting the write".to_string())
    }
}

#[cfg(unix)]
fn run_perl_dcommit(
    work_tree: &Path,
    transport: DcommitTransport,
    config_dir: &Path,
    authors_prog: Option<&Path>,
    expect_failure: bool,
) -> Result<CapturedCommandOutput, String> {
    let mut args = vec![
        "svn".to_string(),
        "dcommit".to_string(),
        "--no-rebase".to_string(),
    ];
    match transport {
        DcommitTransport::File => {
            args.extend(["--username".to_string(), "compat-user".to_string()]);
        }
        DcommitTransport::AuthenticatedSvn => {
            args.extend([
                "--config-dir".to_string(),
                path_arg(config_dir)?.to_string(),
                "--username".to_string(),
                "alice".to_string(),
            ]);
        }
    }
    if let Some(authors_prog) = authors_prog {
        args.extend([
            "--authors-prog".to_string(),
            path_arg(authors_prog)?.to_string(),
        ]);
    }
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    if expect_failure {
        return run_capture_expect_failure(work_tree, "git", &args);
    }
    match transport {
        DcommitTransport::File => run_capture(work_tree, "git", &args),
        DcommitTransport::AuthenticatedSvn => {
            run_capture_with_stdin(work_tree, "git", &args, "secret\nsecret\nsecret\n")
        }
    }
}

#[cfg(unix)]
fn dcommit_shared_fetch_args(transport: DcommitTransport) -> SharedFetchArgs {
    match transport {
        DcommitTransport::File => SharedFetchArgs {
            username: Some("compat-user".to_string()),
            ..default_shared_fetch_args()
        },
        DcommitTransport::AuthenticatedSvn => SharedFetchArgs {
            username: Some("alice".to_string()),
            password: Some("secret".to_string()),
            no_auth_cache: true,
            ..default_shared_fetch_args()
        },
    }
}

#[cfg(unix)]
fn prepare_dcommit_commit(work_tree: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    run(
        work_tree,
        "git",
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    )?;
    fs::create_dir_all(work_tree.join("docs")).map_err(|error| error.to_string())?;
    fs::create_dir_all(work_tree.join("scripts")).map_err(|error| error.to_string())?;
    fs::write(
        work_tree.join("src/lib.rs"),
        "pub fn answer() -> u8 { 43 }\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        work_tree.join("docs/guide.txt"),
        "compatibility guide\nsecond line\n",
    )
    .map_err(|error| error.to_string())?;
    let mut permissions = fs::metadata(work_tree.join("docs/guide.txt"))
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(work_tree.join("docs/guide.txt"), permissions)
        .map_err(|error| error.to_string())?;
    run(work_tree, "git", &["mv", "run.sh", "scripts/run.sh"])?;
    run(work_tree, "git", &["rm", "link-to-lib"])?;
    run(work_tree, "git", &["add", "src/lib.rs", "docs/guide.txt"])?;

    let output = Command::new("git")
        .current_dir(work_tree)
        .env("GIT_AUTHOR_DATE", "2026-07-27T12:00:00+0000")
        .env("GIT_COMMITTER_DATE", "2026-07-27T12:00:00+0000")
        .args([
            "-c",
            "user.name=Compat User",
            "-c",
            "user.email=compat@example.com",
            "commit",
            "-m",
            "compat dcommit subject",
            "-m",
            "compat dcommit body",
        ])
        .output()
        .map_err(|error| format!("git failed to start: {error}"))?;
    if !output.status.success() {
        return Err(command_error("git", output));
    }
    Ok(())
}

#[cfg(unix)]
fn collect_dcommit_write_artifacts(
    work_tree: &Path,
    repository: &Path,
    dcommit_summary: String,
) -> Result<DcommitWriteArtifacts, String> {
    let revision = run_text(work_tree, "svnlook", &["youngest", path_arg(repository)?])?
        .trim()
        .parse::<u32>()
        .map_err(|error| format!("invalid svnlook youngest output: {error}"))?;
    let revision_text = revision.to_string();
    let svn_uuid = run_text(work_tree, "svnlook", &["uuid", path_arg(repository)?])?
        .trim()
        .to_string();
    let svn_author = run_text(
        work_tree,
        "svnlook",
        &["author", "-r", &revision_text, path_arg(repository)?],
    )?;
    let svn_date = run_text(
        work_tree,
        "svnlook",
        &["date", "-r", &revision_text, path_arg(repository)?],
    )?;
    let svn_log = run_text(
        work_tree,
        "svnlook",
        &["log", "-r", &revision_text, path_arg(repository)?],
    )?;
    let svn_revision_metadata = format!(
        "revision: {revision}\nauthor: {}\ndate: {}\nmessage:\n{}",
        svn_author.trim_end(),
        svn_date.trim_end(),
        svn_log.trim_end()
    );
    let svn_changed_paths = run_text(
        work_tree,
        "svnlook",
        &[
            "changed",
            "--copy-info",
            "-r",
            &revision_text,
            path_arg(repository)?,
        ],
    )?;
    let svn_tree = run_text(
        work_tree,
        "svnlook",
        &[
            "tree",
            "--full-paths",
            "-r",
            &revision_text,
            path_arg(repository)?,
        ],
    )?;
    let paths = svn_tree
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty() && *path != "/" && !path.ends_with('/'))
        .collect::<Vec<_>>();
    let mut svn_contents = Vec::new();
    let mut svn_properties = Vec::new();
    for path in paths {
        let contents = run_text(
            work_tree,
            "svnlook",
            &["cat", "-r", &revision_text, path_arg(repository)?, path],
        )?;
        svn_contents.push(format!("{path}\t{}", escape_artifact_value(&contents)));
        let property_names = run_text(
            work_tree,
            "svnlook",
            &[
                "proplist",
                "-r",
                &revision_text,
                path_arg(repository)?,
                path,
            ],
        )?
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
        for name in property_names {
            let value = run_text(
                work_tree,
                "svnlook",
                &[
                    "propget",
                    "-r",
                    &revision_text,
                    path_arg(repository)?,
                    &name,
                    path,
                ],
            )?;
            svn_properties.push(format!("{path}\t{name}\t{}", escape_artifact_value(&value)));
        }
    }
    svn_contents.sort();
    svn_properties.sort();

    let refs = run_text(
        work_tree,
        "git",
        &["for-each-ref", "refs/remotes", "--format=%(refname)"],
    )?
    .lines()
    .map(str::to_string)
    .filter(|line| !line.ends_with("/HEAD"))
    .collect::<Vec<_>>();
    let mut graph_revisions = refs.clone();
    graph_revisions.push("HEAD".to_string());
    let git_svn_id_footers = run_text(
        work_tree,
        "git",
        &[
            "log",
            "--reverse",
            "--format=%B",
            "refs/remotes/origin/trunk",
        ],
    )?
    .lines()
    .filter(|line| line.starts_with("git-svn-id: "))
    .map(str::to_string)
    .collect();

    Ok(DcommitWriteArtifacts {
        svn_revision: revision,
        svn_uuid,
        svn_revision_metadata,
        svn_changed_paths,
        svn_tree,
        svn_contents,
        svn_properties,
        ref_tips: supported_ref_tips(work_tree)?,
        commit_graph: supported_commit_graph(work_tree, &graph_revisions)?,
        clone_state: collect_clone_state(work_tree)?,
        git_svn_id_footers,
        rev_map: supported_rev_map(work_tree, &refs)?,
        rev_map_byte_lengths: supported_rev_map_byte_lengths(work_tree, &refs)?,
        dcommit_summary,
    })
}

#[cfg(unix)]
fn write_dcommit_artifacts(
    capture: &GoldenArtifactCapture,
    tool: &str,
    artifacts: &DcommitWriteArtifacts,
    raw_output: &CapturedCommandOutput,
) -> Result<(), String> {
    capture.write_text(
        &format!("{tool}/dcommit-output.txt"),
        &format!(
            "status: {}\nstdout:\n{}\nstderr:\n{}",
            raw_output.status, raw_output.stdout, raw_output.stderr
        ),
    )?;
    capture.write_text(
        &format!("{tool}/svn-revision-metadata.txt"),
        &artifacts.svn_revision_metadata,
    )?;
    capture.write_text(
        &format!("{tool}/svn-changed-paths.txt"),
        &artifacts.svn_changed_paths,
    )?;
    capture.write_text(&format!("{tool}/svn-tree.txt"), &artifacts.svn_tree)?;
    capture.write_text(
        &format!("{tool}/svn-contents.txt"),
        &artifacts.svn_contents.join("\n"),
    )?;
    capture.write_text(
        &format!("{tool}/svn-properties.txt"),
        &artifacts.svn_properties.join("\n\n"),
    )?;
    capture.write_text(
        &format!("{tool}/ref-tips.txt"),
        &format_ref_tips(&artifacts.ref_tips),
    )?;
    capture.write_text(
        &format!("{tool}/commit-graph.txt"),
        &format_commit_graph(&artifacts.commit_graph),
    )?;
    capture.write_text(
        &format!("{tool}/clone-state.txt"),
        &format_clone_state(&artifacts.clone_state),
    )?;
    capture.write_text(
        &format!("{tool}/git-svn-id-footers.txt"),
        &artifacts.git_svn_id_footers.join("\n"),
    )?;
    capture.write_text(
        &format!("{tool}/rev-map.txt"),
        &format_rev_map(&artifacts.rev_map),
    )?;
    capture.write_text(
        &format!("{tool}/rev-map-byte-lengths.txt"),
        &format_rev_map_byte_lengths(&artifacts.rev_map_byte_lengths),
    )?;
    capture.write_text(
        &format!("{tool}/dcommit-summary.txt"),
        &artifacts.dcommit_summary,
    )?;
    Ok(())
}

#[cfg(unix)]
fn normalize_dcommit_summary(
    output: &CapturedCommandOutput,
    revision: u32,
) -> Result<String, String> {
    if output.status != 0 {
        return Ok(format!("status: nonzero\nrevision: {revision}"));
    }
    let combined = format!("{}\n{}", output.stdout, output.stderr);
    let revision_marker = format!("r{revision}");
    if !combined.contains(&revision_marker) {
        return Err(format!(
            "successful dcommit output did not identify {revision_marker}: {combined:?}"
        ));
    }
    Ok(format!("status: 0\nrevision: {revision}"))
}

#[derive(Clone, Copy)]
enum GoldenLayout {
    Trunk,
    StdLayout,
    Subdirectory,
}

fn run_standard_golden_comparison(
    root: impl AsRef<Path>,
    layout: GoldenLayout,
) -> Result<GoldenComparison, String> {
    let root = root.as_ref();
    let fixture = MaterializedSvnFixture::create(root)?;
    let capture_name = match layout {
        GoldenLayout::Trunk => "standard-linear-history",
        GoldenLayout::StdLayout => "standard-layout-history",
        GoldenLayout::Subdirectory => "standard-subdirectory-history",
    };
    let capture = GoldenArtifactCapture::new(root, capture_name)?;
    let fixture_url = match layout {
        GoldenLayout::Subdirectory => format!("{}/trunk", fixture.url().trim_end_matches('/')),
        GoldenLayout::Trunk | GoldenLayout::StdLayout => fixture.url(),
    };

    let perl_path = root.join("perl-clone");
    let mut perl_clone_args = vec![
        "svn",
        "clone",
        "--preserve-empty-dirs",
        "--placeholder-filename",
        ".gitkeep",
    ];
    match layout {
        GoldenLayout::Trunk => perl_clone_args.extend(["--trunk", "trunk"]),
        GoldenLayout::StdLayout => perl_clone_args.push("--stdlayout"),
        GoldenLayout::Subdirectory => {}
    }
    if !matches!(layout, GoldenLayout::Subdirectory) {
        perl_clone_args.push("--prefix=origin/");
    }
    perl_clone_args.extend([fixture_url.as_str(), path_arg(&perl_path)?]);
    let perl_clone_output = run_capture(root, "git", &perl_clone_args)?;
    let mut perl = collect_supported_artifacts(
        &perl_path,
        GoldenTool::Perl,
        normalize_clone_output(&perl_clone_output),
    )?;
    let perl_no_checkout_path = root.join("perl-clone-no-checkout");
    let mut perl_no_checkout_args = vec!["svn", "clone", "--no-checkout"];
    match layout {
        GoldenLayout::Trunk => perl_no_checkout_args.extend(["--trunk", "trunk"]),
        GoldenLayout::StdLayout => perl_no_checkout_args.push("--stdlayout"),
        GoldenLayout::Subdirectory => {}
    }
    if !matches!(layout, GoldenLayout::Subdirectory) {
        perl_no_checkout_args.push("--prefix=origin/");
    }
    perl_no_checkout_args.extend([fixture_url.as_str(), path_arg(&perl_no_checkout_path)?]);
    run(root, "git", &perl_no_checkout_args)?;
    perl.no_checkout_clone_state = collect_clone_state(&perl_no_checkout_path)?;
    capture.write_text("perl/clone-output.txt", &perl.clone_output)?;
    capture.write_text("perl/config.txt", &format_config(&perl.config))?;
    capture.write_text("perl/refs.txt", &perl.refs.join("\n"))?;
    capture.write_text("perl/ref-tips.txt", &format_ref_tips(&perl.ref_tips))?;
    capture.write_text(
        "perl/commit-graph.txt",
        &format_commit_graph(&perl.commit_graph),
    )?;
    capture.write_text(
        "perl/clone-state.txt",
        &format_clone_state(&perl.clone_state),
    )?;
    capture.write_text(
        "perl/clone-state-no-checkout.txt",
        &format_clone_state(&perl.no_checkout_clone_state),
    )?;
    capture.write_text(
        "perl/git-svn-id-footers.txt",
        &perl.git_svn_id_footers.join("\n"),
    )?;
    capture.write_text("perl/rev-map.txt", &format_rev_map(&perl.rev_map))?;
    capture.write_text(
        "perl/rev-map-byte-lengths.txt",
        &format_rev_map_byte_lengths(&perl.rev_map_byte_lengths),
    )?;
    capture.write_text("perl/file-modes.txt", &format_file_modes(&perl.file_modes))?;
    capture.write_text(
        "perl/file-properties.txt",
        &format_file_properties(&perl.file_properties),
    )?;
    capture.write_text("perl/tree-contents.txt", &perl.tree_contents.join("\n"))?;
    capture.write_text(
        "perl/empty-dir-placeholders.txt",
        &perl.empty_dir_placeholders.join("\n"),
    )?;
    capture.write_text("perl/log-oneline.txt", &perl.log_oneline)?;
    capture.write_text("perl/log-incremental.txt", &perl.log_incremental)?;
    capture.write_text("perl/log-verbose.txt", &perl.log_verbose)?;
    capture.write_text(
        "perl/log-oneline-show-commit.txt",
        &perl.log_oneline_show_commit,
    )?;
    capture.write_text("perl/log-limit-oneline.txt", &perl.log_limit_oneline)?;
    capture.write_text("perl/log-path-oneline.txt", &perl.log_path_oneline)?;
    capture.write_text("perl/find-rev.txt", &perl.find_rev)?;
    capture.write_text("perl/info-url.txt", &perl.info_url)?;
    capture.write_text("perl/info-summary.txt", &perl.info_summary)?;
    capture.write_text("perl/log-revision-oneline.txt", &perl.log_revision_oneline)?;
    capture.write_text(
        "perl/log-revision-range-oneline.txt",
        &perl.log_revision_range_oneline,
    )?;
    capture.write_text(
        "perl/log-revision-reverse-range-oneline.txt",
        &perl.log_revision_reverse_range_oneline,
    )?;
    capture.write_text("perl/find-rev-nearest.txt", &perl.find_rev_nearest)?;
    capture.write_text("perl/find-rev-commit.txt", &perl.find_rev_commit)?;
    capture.write_text("perl/rebase-dry-run.txt", &perl.rebase_dry_run)?;
    capture.write_text("perl/reset.txt", &perl.reset)?;
    capture.write_text("perl/gc-output.txt", &perl.gc_output)?;

    let rust_path = root.join("rust-clone");
    let rust_clone_output = commands::clone::run_with_output(CloneArgs {
        url: fixture_url.clone(),
        path: Some(path_arg(&rust_path)?.to_string()),
        layout: golden_layout(layout),
        shared: default_shared_fetch_args(),
        no_checkout: false,
    })?;
    let mut rust = collect_supported_artifacts(
        &rust_path,
        GoldenTool::Rust,
        normalize_clone_output(&CapturedCommandOutput {
            status: 0,
            stdout: rust_clone_output.stdout,
            stderr: rust_clone_output.stderr,
        }),
    )?;
    let rust_no_checkout_path = root.join("rust-clone-no-checkout");
    commands::clone::run(CloneArgs {
        url: fixture_url,
        path: Some(path_arg(&rust_no_checkout_path)?.to_string()),
        layout: golden_layout(layout),
        shared: default_shared_fetch_args(),
        no_checkout: true,
    })?;
    rust.no_checkout_clone_state = collect_clone_state(&rust_no_checkout_path)?;
    capture.write_text("rust/clone-output.txt", &rust.clone_output)?;
    capture.write_text("rust/config.txt", &format_config(&rust.config))?;
    capture.write_text("rust/refs.txt", &rust.refs.join("\n"))?;
    capture.write_text("rust/ref-tips.txt", &format_ref_tips(&rust.ref_tips))?;
    capture.write_text(
        "rust/commit-graph.txt",
        &format_commit_graph(&rust.commit_graph),
    )?;
    capture.write_text(
        "rust/clone-state.txt",
        &format_clone_state(&rust.clone_state),
    )?;
    capture.write_text(
        "rust/clone-state-no-checkout.txt",
        &format_clone_state(&rust.no_checkout_clone_state),
    )?;
    capture.write_text(
        "rust/git-svn-id-footers.txt",
        &rust.git_svn_id_footers.join("\n"),
    )?;
    capture.write_text("rust/rev-map.txt", &format_rev_map(&rust.rev_map))?;
    capture.write_text(
        "rust/rev-map-byte-lengths.txt",
        &format_rev_map_byte_lengths(&rust.rev_map_byte_lengths),
    )?;
    capture.write_text("rust/file-modes.txt", &format_file_modes(&rust.file_modes))?;
    capture.write_text(
        "rust/file-properties.txt",
        &format_file_properties(&rust.file_properties),
    )?;
    capture.write_text("rust/tree-contents.txt", &rust.tree_contents.join("\n"))?;
    capture.write_text(
        "rust/empty-dir-placeholders.txt",
        &rust.empty_dir_placeholders.join("\n"),
    )?;
    capture.write_text("rust/log-oneline.txt", &rust.log_oneline)?;
    capture.write_text("rust/log-incremental.txt", &rust.log_incremental)?;
    capture.write_text("rust/log-verbose.txt", &rust.log_verbose)?;
    capture.write_text(
        "rust/log-oneline-show-commit.txt",
        &rust.log_oneline_show_commit,
    )?;
    capture.write_text("rust/log-limit-oneline.txt", &rust.log_limit_oneline)?;
    capture.write_text("rust/log-path-oneline.txt", &rust.log_path_oneline)?;
    capture.write_text("rust/find-rev.txt", &rust.find_rev)?;
    capture.write_text("rust/info-url.txt", &rust.info_url)?;
    capture.write_text("rust/info-summary.txt", &rust.info_summary)?;
    capture.write_text("rust/log-revision-oneline.txt", &rust.log_revision_oneline)?;
    capture.write_text(
        "rust/log-revision-range-oneline.txt",
        &rust.log_revision_range_oneline,
    )?;
    capture.write_text(
        "rust/log-revision-reverse-range-oneline.txt",
        &rust.log_revision_reverse_range_oneline,
    )?;
    capture.write_text("rust/find-rev-nearest.txt", &rust.find_rev_nearest)?;
    capture.write_text("rust/find-rev-commit.txt", &rust.find_rev_commit)?;
    capture.write_text("rust/rebase-dry-run.txt", &rust.rebase_dry_run)?;
    capture.write_text("rust/reset.txt", &rust.reset)?;
    capture.write_text("rust/gc-output.txt", &rust.gc_output)?;

    Ok(GoldenComparison {
        perl,
        rust,
        capture,
    })
}

fn golden_layout(layout: GoldenLayout) -> LayoutArgs {
    LayoutArgs {
        stdlayout: matches!(layout, GoldenLayout::StdLayout),
        trunk: matches!(layout, GoldenLayout::Trunk).then(|| "trunk".to_string()),
        branches: Vec::new(),
        tags: Vec::new(),
        prefix: None,
    }
}

pub fn compare_supported_subset(
    perl: &GoldenComparisonArtifacts,
    rust: &GoldenComparisonArtifacts,
) -> Result<(), String> {
    let mut mismatches = Vec::new();

    if perl.config != rust.config {
        mismatches.push(format!(
            "config differs\nperl: {:?}\nrust: {:?}",
            perl.config, rust.config
        ));
    }
    if perl.refs != rust.refs {
        mismatches.push(format!(
            "refs differ\nperl: {:?}\nrust: {:?}",
            perl.refs, rust.refs
        ));
    }
    if perl.ref_tips != rust.ref_tips {
        mismatches.push(format!(
            "ref tips differ\nperl: {:?}\nrust: {:?}",
            perl.ref_tips, rust.ref_tips
        ));
    }
    if perl.commit_graph != rust.commit_graph {
        mismatches.push(format!(
            "commit graph differs\nperl: {:?}\nrust: {:?}",
            perl.commit_graph, rust.commit_graph
        ));
    }
    if perl.clone_state != rust.clone_state {
        mismatches.push(format!(
            "clone state differs\nperl: {:?}\nrust: {:?}",
            perl.clone_state, rust.clone_state
        ));
    }
    if perl.no_checkout_clone_state != rust.no_checkout_clone_state {
        mismatches.push(format!(
            "--no-checkout clone state differs\nperl: {:?}\nrust: {:?}",
            perl.no_checkout_clone_state, rust.no_checkout_clone_state
        ));
    }
    if perl.git_svn_id_footers != rust.git_svn_id_footers {
        mismatches.push(format!(
            "git-svn-id footers differ\nperl: {:?}\nrust: {:?}",
            perl.git_svn_id_footers, rust.git_svn_id_footers
        ));
    }
    if perl.rev_map != rust.rev_map {
        mismatches.push(format!(
            "rev_map records differ\nperl: {:?}\nrust: {:?}",
            perl.rev_map, rust.rev_map
        ));
    }
    if perl.rev_map_byte_lengths != rust.rev_map_byte_lengths {
        mismatches.push(format!(
            "rev_map byte lengths differ\nperl: {:?}\nrust: {:?}",
            perl.rev_map_byte_lengths, rust.rev_map_byte_lengths
        ));
    }
    if perl.file_modes != rust.file_modes {
        mismatches.push(format!(
            "file modes differ\nperl: {:?}\nrust: {:?}",
            perl.file_modes, rust.file_modes
        ));
    }
    if perl.file_properties != rust.file_properties {
        mismatches.push(format!(
            "file properties differ\nperl: {:?}\nrust: {:?}",
            perl.file_properties, rust.file_properties
        ));
    }
    if perl.tree_contents != rust.tree_contents {
        mismatches.push(format!(
            "tree contents differ\nperl: {:?}\nrust: {:?}",
            perl.tree_contents, rust.tree_contents
        ));
    }
    if perl.empty_dir_placeholders != rust.empty_dir_placeholders {
        mismatches.push(format!(
            "empty dir placeholders differ\nperl: {:?}\nrust: {:?}",
            perl.empty_dir_placeholders, rust.empty_dir_placeholders
        ));
    }
    if perl.log_oneline != rust.log_oneline {
        mismatches.push(format!(
            "log --oneline differs\nperl: {:?}\nrust: {:?}",
            perl.log_oneline, rust.log_oneline
        ));
    }
    if perl.log_incremental != rust.log_incremental {
        mismatches.push(format!(
            "log --incremental differs\nperl: {:?}\nrust: {:?}",
            perl.log_incremental, rust.log_incremental
        ));
    }
    if perl.log_verbose != rust.log_verbose {
        mismatches.push(format!(
            "log --verbose differs\nperl: {:?}\nrust: {:?}",
            perl.log_verbose, rust.log_verbose
        ));
    }
    if perl.log_oneline_show_commit != rust.log_oneline_show_commit {
        mismatches.push(format!(
            "log --oneline --show-commit differs\nperl: {:?}\nrust: {:?}",
            perl.log_oneline_show_commit, rust.log_oneline_show_commit
        ));
    }
    if perl.log_limit_oneline != rust.log_limit_oneline {
        mismatches.push(format!(
            "log --limit output differs\nperl: {:?}\nrust: {:?}",
            perl.log_limit_oneline, rust.log_limit_oneline
        ));
    }
    if perl.log_path_oneline != rust.log_path_oneline {
        mismatches.push(format!(
            "log pathspec output differs\nperl: {:?}\nrust: {:?}",
            perl.log_path_oneline, rust.log_path_oneline
        ));
    }
    if perl.find_rev != rust.find_rev {
        mismatches.push(format!(
            "find-rev output differs\nperl: {:?}\nrust: {:?}",
            perl.find_rev, rust.find_rev
        ));
    }
    if perl.info_url != rust.info_url {
        mismatches.push(format!(
            "info --url output differs\nperl: {:?}\nrust: {:?}",
            perl.info_url, rust.info_url
        ));
    }
    if perl.info_summary != rust.info_summary {
        mismatches.push(format!(
            "info output differs\nperl: {:?}\nrust: {:?}",
            perl.info_summary, rust.info_summary
        ));
    }
    if perl.log_revision_oneline != rust.log_revision_oneline {
        mismatches.push(format!(
            "log --revision output differs\nperl: {:?}\nrust: {:?}",
            perl.log_revision_oneline, rust.log_revision_oneline
        ));
    }
    if perl.log_revision_range_oneline != rust.log_revision_range_oneline {
        mismatches.push(format!(
            "log --revision range output differs\nperl: {:?}\nrust: {:?}",
            perl.log_revision_range_oneline, rust.log_revision_range_oneline
        ));
    }
    if perl.log_revision_reverse_range_oneline != rust.log_revision_reverse_range_oneline {
        mismatches.push(format!(
            "log --revision reverse range output differs\nperl: {:?}\nrust: {:?}",
            perl.log_revision_reverse_range_oneline, rust.log_revision_reverse_range_oneline
        ));
    }
    if perl.find_rev_nearest != rust.find_rev_nearest {
        mismatches.push(format!(
            "find-rev nearest output differs\nperl: {:?}\nrust: {:?}",
            perl.find_rev_nearest, rust.find_rev_nearest
        ));
    }
    if perl.find_rev_commit != rust.find_rev_commit {
        mismatches.push(format!(
            "find-rev commit output differs\nperl: {:?}\nrust: {:?}",
            perl.find_rev_commit, rust.find_rev_commit
        ));
    }
    if perl.rebase_dry_run != rust.rebase_dry_run {
        mismatches.push(format!(
            "rebase --dry-run output differs\nperl: {:?}\nrust: {:?}",
            perl.rebase_dry_run, rust.rebase_dry_run
        ));
    }
    if perl.reset != rust.reset {
        mismatches.push(format!(
            "reset output differs\nperl: {:?}\nrust: {:?}",
            perl.reset, rust.reset
        ));
    }
    if perl.gc_output != rust.gc_output {
        mismatches.push(format!(
            "gc output differs\nperl: {:?}\nrust: {:?}",
            perl.gc_output, rust.gc_output
        ));
    }
    if perl.clone_output != rust.clone_output {
        mismatches.push(format!(
            "clone output differs\nperl: {:?}\nrust: {:?}",
            perl.clone_output, rust.clone_output
        ));
    }

    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(mismatches.join("\n\n"))
    }
}

fn strict_compat() -> bool {
    std::env::var("GIT_SVN_RS_STRICT_COMPAT").as_deref() == Ok("1")
}

fn has_svn_tools() -> bool {
    command_succeeds("svnadmin", &["--version"]) && command_succeeds("svn", &["--version"])
}

fn command_succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

struct MaterializedSvnFixture {
    repo: PathBuf,
}

impl MaterializedSvnFixture {
    fn create(root: &Path) -> Result<Self, String> {
        let repo = root.join("svn-repo");
        let wc = root.join("svn-wc");

        run(root, "svnadmin", &["create", path_arg(&repo)?])?;

        let url = file_url(&repo)?;
        run(
            root,
            "svn",
            &["checkout", "--non-interactive", &url, path_arg(&wc)?],
        )?;
        run(
            &wc,
            "svn",
            &["mkdir", "--non-interactive", "trunk", "branches", "tags"],
        )?;
        run(&wc, "svn", &["commit", "--non-interactive", "-m", "layout"])?;

        fs::create_dir_all(wc.join("trunk/src")).map_err(|e| e.to_string())?;
        fs::write(
            wc.join("trunk/src/lib.rs"),
            "pub fn answer() -> u8 { 42 }\n",
        )
        .map_err(|e| e.to_string())?;
        fs::write(wc.join("trunk/run.sh"), "#!/bin/sh\necho hi\n").map_err(|e| e.to_string())?;
        #[cfg(unix)]
        std::os::unix::fs::symlink("src/lib.rs", wc.join("trunk/link-to-lib"))
            .map_err(|e| e.to_string())?;
        #[cfg(not(unix))]
        fs::write(wc.join("trunk/link-to-lib"), "link src/lib.rs").map_err(|e| e.to_string())?;
        fs::write(wc.join("trunk/deleted.txt"), "temporary\n").map_err(|e| e.to_string())?;
        run(
            &wc,
            "svn",
            &[
                "add",
                "--non-interactive",
                "trunk/src",
                "trunk/run.sh",
                "trunk/link-to-lib",
                "trunk/deleted.txt",
            ],
        )?;
        run(
            &wc,
            "svn",
            &[
                "propset",
                "--non-interactive",
                "svn:executable",
                "x",
                "trunk/run.sh",
            ],
        )?;
        #[cfg(not(unix))]
        run(
            &wc,
            "svn",
            &[
                "propset",
                "--non-interactive",
                "svn:special",
                "x",
                "trunk/link-to-lib",
            ],
        )?;
        for (name, value) in [
            ("svn:eol-style", "LF"),
            ("svn:mime-type", "text/plain"),
            ("svn:keywords", "Id"),
            ("svn:needs-lock", "x"),
        ] {
            run(
                &wc,
                "svn",
                &[
                    "propset",
                    "--non-interactive",
                    name,
                    value,
                    "trunk/src/lib.rs",
                ],
            )?;
        }
        run(
            &wc,
            "svn",
            &["commit", "--non-interactive", "-m", "add trunk file"],
        )?;

        run(
            &wc,
            "svn",
            &["mkdir", "--non-interactive", "trunk/empty-dir"],
        )?;
        run(
            &wc,
            "svn",
            &["commit", "--non-interactive", "-m", "add empty directory"],
        )?;

        run(
            &wc,
            "svn",
            &["copy", "--non-interactive", "trunk", "branches/main"],
        )?;
        run(
            &wc,
            "svn",
            &["commit", "--non-interactive", "-m", "branch main"],
        )?;

        run(
            &wc,
            "svn",
            &["copy", "--non-interactive", "trunk", "tags/v1"],
        )?;
        run(&wc, "svn", &["commit", "--non-interactive", "-m", "tag v1"])?;

        run(
            &wc,
            "svn",
            &["delete", "--non-interactive", "trunk/deleted.txt"],
        )?;
        run(
            &wc,
            "svn",
            &["commit", "--non-interactive", "-m", "delete temporary file"],
        )?;

        Ok(Self { repo })
    }

    fn url(&self) -> String {
        file_url(&self.repo).expect("fixture repository path should convert to file URL")
    }
}

#[derive(Debug, Clone, Copy)]
enum GoldenTool {
    Perl,
    Rust,
}

fn collect_supported_artifacts(
    work_tree: &Path,
    tool: GoldenTool,
    clone_output: String,
) -> Result<GoldenComparisonArtifacts, String> {
    let refs = run_text(
        work_tree,
        "git",
        &["for-each-ref", "refs/remotes", "--format=%(refname)"],
    )?
    .lines()
    .map(str::to_string)
    .filter(|line| !line.ends_with("/HEAD"))
    .collect::<Vec<_>>();
    let ref_tips = supported_ref_tips(work_tree)?;
    let commit_graph = supported_commit_graph(work_tree, &refs)?;
    let clone_state = collect_clone_state(work_tree)?;

    let config = supported_config(work_tree)?;
    let head_object_id = clone_state
        .head_object_id
        .as_deref()
        .ok_or_else(|| "golden clone did not create HEAD".to_string())?;
    let matching_refs = refs
        .iter()
        .filter_map(|source_ref| {
            run_text(work_tree, "git", &["rev-parse", source_ref])
                .ok()
                .filter(|object_id| object_id.trim() == head_object_id)
                .map(|_| source_ref)
        })
        .collect::<Vec<_>>();
    let rev = match matching_refs.as_slice() {
        [source_ref] => *source_ref,
        [] => return Err("golden clone HEAD does not match a remote ref".to_string()),
        _ => {
            return Err(format!(
                "golden clone HEAD ambiguously matches remote refs: {}",
                matching_refs
                    .iter()
                    .map(|source_ref| source_ref.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    };
    let git_svn_id_footers = run_text(work_tree, "git", &["log", "--reverse", "--format=%B", rev])?
        .lines()
        .filter(|line| line.starts_with("git-svn-id: "))
        .map(str::to_string)
        .collect::<Vec<_>>();
    let rev_map = supported_rev_map(work_tree, &refs)?;
    let rev_map_byte_lengths = supported_rev_map_byte_lengths(work_tree, &refs)?;
    let file_modes = supported_file_modes(work_tree, rev)?;
    let tree_contents = supported_tree_contents(work_tree, rev, &file_modes)?;
    let empty_dir_placeholders = supported_empty_dir_placeholders(work_tree, rev)?;
    let first_revision = rev_map
        .iter()
        .find(|record| record.source_ref == *rev && record.object_id.is_some())
        .ok_or_else(|| "golden clone did not write a rev_map record".to_string())?
        .revision;
    let log_oneline = supported_log_oneline(work_tree, tool)?;
    let log_incremental = supported_log_incremental(work_tree, tool)?;
    let log_verbose = supported_log_verbose(work_tree, tool)?;
    let log_oneline_show_commit = supported_log_oneline_show_commit(work_tree, tool)?;
    let log_limit_oneline = supported_log_limit_oneline(work_tree, tool)?;
    let log_path_oneline = supported_log_path_oneline(work_tree, tool)?;
    let find_rev = supported_find_rev(work_tree, tool, first_revision)?;
    let info_url = supported_info_url(work_tree, tool)?;
    let file_properties =
        supported_file_properties(work_tree, &info_url, &file_modes, &empty_dir_placeholders)?;
    let info_summary = supported_info_summary(work_tree, tool)?;
    let log_revision_oneline = supported_log_revision_oneline(work_tree, tool, first_revision)?;
    let log_revision_range_oneline =
        supported_log_revision_range_oneline(work_tree, tool, first_revision, first_revision + 1)?;
    let log_revision_reverse_range_oneline =
        supported_log_revision_range_oneline(work_tree, tool, first_revision + 1, first_revision)?;
    let find_rev_nearest = supported_find_rev_nearest(work_tree, tool, first_revision + 1)?;
    let find_rev_commit = supported_find_rev_commit(work_tree, tool, rev, first_revision)?;
    let rebase_dry_run = supported_rebase_dry_run(work_tree, tool)?;
    let reset = supported_reset(work_tree, tool, first_revision + 1)?;
    let gc_output = supported_gc(work_tree, tool)?;

    Ok(GoldenComparisonArtifacts {
        config,
        refs,
        ref_tips,
        commit_graph,
        clone_state,
        no_checkout_clone_state: CloneStateArtifact::default(),
        git_svn_id_footers,
        rev_map,
        rev_map_byte_lengths,
        file_modes,
        file_properties,
        tree_contents,
        empty_dir_placeholders,
        log_oneline,
        log_incremental,
        log_verbose,
        log_oneline_show_commit,
        log_limit_oneline,
        log_path_oneline,
        find_rev,
        info_url,
        info_summary,
        log_revision_oneline,
        log_revision_range_oneline,
        log_revision_reverse_range_oneline,
        find_rev_nearest,
        find_rev_commit,
        rebase_dry_run,
        reset,
        gc_output,
        clone_output,
    })
}

fn collect_clone_state(work_tree: &Path) -> Result<CloneStateArtifact, String> {
    let head_symbolic_ref = optional_git_text(work_tree, &["symbolic-ref", "-q", "HEAD"])?;
    let head_object_id = optional_git_text(work_tree, &["rev-parse", "--verify", "HEAD"])?;
    let mut local_branches = run_text(
        work_tree,
        "git",
        &[
            "for-each-ref",
            "refs/heads",
            "--format=%(refname)\t%(objectname)\t%(upstream)",
        ],
    )?
    .lines()
    .map(str::to_string)
    .collect::<Vec<_>>();
    local_branches.sort();

    let mut index_entries = run_text(work_tree, "git", &["ls-files", "--stage", "-z"])?
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    index_entries.sort();

    let mut worktree_entries = Vec::new();
    for path in run_text(work_tree, "git", &["ls-files", "-z"])?
        .split('\0')
        .filter(|path| !path.is_empty())
    {
        let content = fs::read(work_tree.join(path))
            .map_err(|error| format!("failed to read worktree path {path:?}: {error}"))?;
        worktree_entries.push(format!("{path}\t{}", hex::encode(content)));
    }
    worktree_entries.sort();

    let status_porcelain_v2 = run_text(
        work_tree,
        "git",
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=all",
        ],
    )?
    .replace('\0', "\n");

    Ok(CloneStateArtifact {
        head_symbolic_ref,
        head_object_id,
        local_branches,
        index_entries,
        worktree_entries,
        status_porcelain_v2,
    })
}

fn optional_git_text(work_tree: &Path, args: &[&str]) -> Result<Option<String>, String> {
    let output = Command::new("git")
        .current_dir(work_tree)
        .args(args)
        .output()
        .map_err(|error| format!("git failed to start: {error}"))?;
    if output.status.success() {
        return Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ));
    }
    if output.status.code() == Some(1) || output.status.code() == Some(128) {
        return Ok(None);
    }
    Err(command_error("git", output))
}

fn supported_ref_tips(work_tree: &Path) -> Result<Vec<RefTipArtifact>, String> {
    let mut refs = run_text(
        work_tree,
        "git",
        &[
            "for-each-ref",
            "refs/remotes",
            "--format=%(refname)\t%(objectname)",
        ],
    )?
    .lines()
    .filter(|line| !line.starts_with("refs/remotes/") || !line.contains("/HEAD\t"))
    .map(|line| {
        let (name, object_id) = line
            .split_once('\t')
            .ok_or_else(|| format!("invalid ref artifact: {line}"))?;
        Ok(RefTipArtifact {
            name: name.to_string(),
            object_id: object_id.to_string(),
        })
    })
    .collect::<Result<Vec<_>, String>>()?;
    refs.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(refs)
}

fn supported_commit_graph(
    work_tree: &Path,
    refs: &[String],
) -> Result<Vec<CommitGraphArtifact>, String> {
    let mut args = vec!["rev-list".to_string()];
    args.extend(refs.iter().cloned());
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    let mut object_ids = run_text(work_tree, "git", &args)?
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
    object_ids.sort();
    object_ids.dedup();

    object_ids
        .into_iter()
        .map(|object_id| {
            let raw = run_text(
                work_tree,
                "git",
                &[
                    "show",
                    "-s",
                    "--format=%H%x00%P%x00%T%x00%an%x00%ae%x00%at%x00%aI%x00%cn%x00%ce%x00%ct%x00%cI%x00%B",
                    &object_id,
                ],
            )?;
            let fields = raw.splitn(12, '\0').collect::<Vec<_>>();
            if fields.len() != 12 {
                return Err(format!("invalid commit graph artifact for {object_id}"));
            }
            Ok(CommitGraphArtifact {
                object_id: fields[0].to_string(),
                parents: fields[1].split_whitespace().map(str::to_string).collect(),
                tree_id: fields[2].to_string(),
                author_name: fields[3].to_string(),
                author_email: fields[4].to_string(),
                author_epoch: fields[5].parse().map_err(|error| {
                    format!("invalid author epoch for {object_id}: {error}")
                })?,
                author_offset: iso8601_offset(fields[6])?,
                committer_name: fields[7].to_string(),
                committer_email: fields[8].to_string(),
                committer_epoch: fields[9].parse().map_err(|error| {
                    format!("invalid committer epoch for {object_id}: {error}")
                })?,
                committer_offset: iso8601_offset(fields[10])?,
                message: fields[11].to_string(),
            })
        })
        .collect()
}

fn iso8601_offset(value: &str) -> Result<String, String> {
    if value.ends_with('Z') {
        return Ok("+0000".to_string());
    }
    let offset = value
        .get(value.len().saturating_sub(6)..)
        .ok_or_else(|| format!("invalid ISO-8601 timestamp: {value}"))?;
    if !matches!(offset.as_bytes(), [b'+' | b'-', _, _, b':', _, _]) {
        return Err(format!("invalid ISO-8601 offset: {value}"));
    }
    Ok(offset.replace(':', ""))
}

fn supported_gc(work_tree: &Path, tool: GoldenTool) -> Result<String, String> {
    match tool {
        GoldenTool::Perl => run(work_tree, "git", &["svn", "gc"])?,
        GoldenTool::Rust => commands::gc::run_in_work_tree(work_tree)?,
    }
    Ok("gc: success".to_string())
}

fn supported_reset(work_tree: &Path, tool: GoldenTool, revision: u32) -> Result<String, String> {
    let parent = work_tree
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", work_tree.display()))?;
    let reset_tree = parent.join(format!(
        "{}-reset",
        work_tree.file_name().unwrap().to_string_lossy()
    ));
    if reset_tree.exists() {
        fs::remove_dir_all(&reset_tree).map_err(|e| e.to_string())?;
    }
    copy_dir_all(work_tree, &reset_tree)?;

    match tool {
        GoldenTool::Perl => {
            run(
                &reset_tree,
                "git",
                &["svn", "reset", "-r", &revision.to_string()],
            )?;
        }
        GoldenTool::Rust => {
            commands::reset::run_in_work_tree(
                &reset_tree,
                ResetArgs {
                    revision: revision.to_string(),
                    parent: false,
                },
            )?;
        }
    }

    let next_revision = revision + 1;
    let after = supported_find_rev(&reset_tree, tool, next_revision)?;
    Ok(format!("reset r{revision}\n{after}"))
}

fn supported_rebase_dry_run(work_tree: &Path, tool: GoldenTool) -> Result<String, String> {
    let output = match tool {
        GoldenTool::Perl => run_text(work_tree, "git", &["svn", "rebase", "--dry-run"])?,
        GoldenTool::Rust => commands::rebase::run_in_work_tree(
            work_tree,
            RebaseArgs {
                dry_run: true,
                local: false,
                merge: false,
                strategy: None,
                shared: default_shared_fetch_args(),
            },
        )?,
    };
    Ok(normalize_rebase_dry_run(&output))
}

fn supported_log_oneline(work_tree: &Path, tool: GoldenTool) -> Result<String, String> {
    let output = match tool {
        GoldenTool::Perl => run_text(work_tree, "git", &["svn", "log", "--oneline"])?,
        GoldenTool::Rust => commands::log::run_in_work_tree(
            work_tree,
            LogArgs {
                revision: None,
                authors_file: None,
                non_recursive: false,
                limit: None,
                verbose: false,
                incremental: false,
                oneline: true,
                show_commit: false,
                git_log_args: Vec::new(),
            },
        )?,
    };
    Ok(normalize_log_oneline(&output))
}

fn supported_log_oneline_show_commit(work_tree: &Path, tool: GoldenTool) -> Result<String, String> {
    let output = match tool {
        GoldenTool::Perl => run_text(
            work_tree,
            "git",
            &["svn", "log", "--oneline", "--show-commit"],
        )?,
        GoldenTool::Rust => commands::log::run_in_work_tree(
            work_tree,
            LogArgs {
                revision: None,
                authors_file: None,
                non_recursive: false,
                limit: None,
                verbose: false,
                incremental: false,
                oneline: true,
                show_commit: true,
                git_log_args: Vec::new(),
            },
        )?,
    };
    Ok(normalize_oneline_show_commit_log(&output))
}

fn supported_log_limit_oneline(work_tree: &Path, tool: GoldenTool) -> Result<String, String> {
    let output = match tool {
        GoldenTool::Perl => run_text(
            work_tree,
            "git",
            &["svn", "log", "--limit", "1", "--oneline"],
        )?,
        GoldenTool::Rust => commands::log::run_in_work_tree(
            work_tree,
            LogArgs {
                revision: None,
                authors_file: None,
                non_recursive: false,
                limit: Some(1),
                verbose: false,
                incremental: false,
                oneline: true,
                show_commit: false,
                git_log_args: Vec::new(),
            },
        )?,
    };
    Ok(normalize_log_oneline(&output))
}

fn supported_log_path_oneline(work_tree: &Path, tool: GoldenTool) -> Result<String, String> {
    let output = match tool {
        GoldenTool::Perl => run_text(
            work_tree,
            "git",
            &["svn", "log", "--oneline", "--", "src/lib.rs"],
        )?,
        GoldenTool::Rust => commands::log::run_in_work_tree(
            work_tree,
            LogArgs {
                revision: None,
                authors_file: None,
                non_recursive: false,
                limit: None,
                verbose: false,
                incremental: false,
                oneline: true,
                show_commit: false,
                git_log_args: vec!["src/lib.rs".to_string()],
            },
        )?,
    };
    Ok(normalize_log_oneline(&output))
}

fn supported_log_verbose(work_tree: &Path, tool: GoldenTool) -> Result<String, String> {
    let output = match tool {
        GoldenTool::Perl => run_text(work_tree, "git", &["svn", "log", "--verbose"])?,
        GoldenTool::Rust => commands::log::run_in_work_tree(
            work_tree,
            LogArgs {
                revision: None,
                authors_file: None,
                non_recursive: false,
                limit: None,
                verbose: true,
                incremental: false,
                oneline: false,
                show_commit: false,
                git_log_args: Vec::new(),
            },
        )?,
    };
    Ok(normalize_verbose_log(&output))
}

fn supported_log_incremental(work_tree: &Path, tool: GoldenTool) -> Result<String, String> {
    let output = match tool {
        GoldenTool::Perl => run_text(work_tree, "git", &["svn", "log", "--incremental"])?,
        GoldenTool::Rust => commands::log::run_in_work_tree(
            work_tree,
            LogArgs {
                revision: None,
                authors_file: None,
                non_recursive: false,
                limit: None,
                verbose: false,
                incremental: true,
                oneline: false,
                show_commit: false,
                git_log_args: Vec::new(),
            },
        )?,
    };
    Ok(normalize_incremental_log(&output))
}

fn supported_log_revision_oneline(
    work_tree: &Path,
    tool: GoldenTool,
    revision: u32,
) -> Result<String, String> {
    let output = match tool {
        GoldenTool::Perl => run_text(
            work_tree,
            "git",
            &[
                "svn",
                "log",
                "--revision",
                &revision.to_string(),
                "--oneline",
            ],
        )?,
        GoldenTool::Rust => commands::log::run_in_work_tree(
            work_tree,
            LogArgs {
                revision: Some(format!("r{revision}")),
                authors_file: None,
                non_recursive: false,
                limit: None,
                verbose: false,
                incremental: false,
                oneline: true,
                show_commit: false,
                git_log_args: Vec::new(),
            },
        )?,
    };
    Ok(normalize_log_oneline(&output))
}

fn supported_log_revision_range_oneline(
    work_tree: &Path,
    tool: GoldenTool,
    start_revision: u32,
    end_revision: u32,
) -> Result<String, String> {
    let output = match tool {
        GoldenTool::Perl => run_text(
            work_tree,
            "git",
            &[
                "svn",
                "log",
                "--revision",
                &format!("{start_revision}:{end_revision}"),
                "--oneline",
            ],
        )?,
        GoldenTool::Rust => commands::log::run_in_work_tree(
            work_tree,
            LogArgs {
                revision: Some(format!("r{start_revision}:r{end_revision}")),
                authors_file: None,
                non_recursive: false,
                limit: None,
                verbose: false,
                incremental: false,
                oneline: true,
                show_commit: false,
                git_log_args: Vec::new(),
            },
        )?,
    };
    Ok(normalize_log_oneline(&output))
}

fn supported_find_rev(work_tree: &Path, tool: GoldenTool, revision: u32) -> Result<String, String> {
    let revision_arg = format!("r{revision}");
    let output = match tool {
        GoldenTool::Perl => run_text(work_tree, "git", &["svn", "find-rev", &revision_arg])?,
        GoldenTool::Rust => commands::find_rev::run_in_work_tree(
            work_tree,
            FindRevArgs {
                rev_or_commit: revision_arg,
                treeish: None,
                before: false,
                after: false,
            },
        )?,
    };
    Ok(normalize_find_rev_output(revision, &output))
}

fn supported_find_rev_nearest(
    work_tree: &Path,
    tool: GoldenTool,
    revision: u32,
) -> Result<String, String> {
    let before =
        supported_find_rev_with_direction(work_tree, tool, revision, FindRevDirection::Before)?;
    let after =
        supported_find_rev_with_direction(work_tree, tool, revision, FindRevDirection::After)?;
    Ok(format!("before {before}\nafter {after}"))
}

fn supported_find_rev_commit(
    work_tree: &Path,
    tool: GoldenTool,
    refname: &str,
    expected_revision: u32,
) -> Result<String, String> {
    let commit = run_text(
        work_tree,
        "git",
        &["rev-list", "--reverse", "-n", "1", refname],
    )?
    .trim()
    .to_string();
    if commit.is_empty() {
        return Err(format!("no commit found for {refname}"));
    }
    let output = match tool {
        GoldenTool::Perl => run_text(work_tree, "git", &["svn", "find-rev", &commit])?,
        GoldenTool::Rust => commands::find_rev::run_in_work_tree(
            work_tree,
            FindRevArgs {
                rev_or_commit: commit.clone(),
                treeish: None,
                before: false,
                after: false,
            },
        )?,
    };
    Ok(format!(
        "{commit} -> {}",
        normalize_commit_to_revision_output(&output, expected_revision)
    ))
}

fn normalize_commit_to_revision_output(output: &str, expected_revision: u32) -> String {
    let trimmed = output.trim();
    if trimmed == expected_revision.to_string() {
        format!("r{trimmed}")
    } else {
        trimmed.to_string()
    }
}

#[derive(Debug, Clone, Copy)]
enum FindRevDirection {
    Before,
    After,
}

fn supported_find_rev_with_direction(
    work_tree: &Path,
    tool: GoldenTool,
    revision: u32,
    direction: FindRevDirection,
) -> Result<String, String> {
    let revision_arg = format!("r{revision}");
    let output = match (tool, direction) {
        (GoldenTool::Perl, FindRevDirection::Before) => run_text(
            work_tree,
            "git",
            &["svn", "find-rev", "--before", &revision_arg],
        )?,
        (GoldenTool::Perl, FindRevDirection::After) => run_text(
            work_tree,
            "git",
            &["svn", "find-rev", "--after", &revision_arg],
        )?,
        (GoldenTool::Rust, FindRevDirection::Before) => commands::find_rev::run_in_work_tree(
            work_tree,
            FindRevArgs {
                rev_or_commit: revision_arg,
                treeish: None,
                before: true,
                after: false,
            },
        )?,
        (GoldenTool::Rust, FindRevDirection::After) => commands::find_rev::run_in_work_tree(
            work_tree,
            FindRevArgs {
                rev_or_commit: revision_arg,
                treeish: None,
                before: false,
                after: true,
            },
        )?,
    };
    Ok(normalize_find_rev_output(revision, &output))
}

fn supported_info_url(work_tree: &Path, tool: GoldenTool) -> Result<String, String> {
    let output = match tool {
        GoldenTool::Perl => run_text(work_tree, "git", &["svn", "info", "--url"])?,
        GoldenTool::Rust => commands::info::run_in_work_tree(work_tree, InfoArgs { url: true })?,
    };
    Ok(output.trim().replace('\\', "/"))
}

fn supported_info_summary(work_tree: &Path, tool: GoldenTool) -> Result<String, String> {
    let output = match tool {
        GoldenTool::Perl => run_text(work_tree, "git", &["svn", "info"])?,
        GoldenTool::Rust => commands::info::run_in_work_tree(work_tree, InfoArgs { url: false })?,
    };
    Ok(normalize_info_summary(&output))
}

fn normalize_info_summary(output: &str) -> String {
    let supported = ["URL:", "Repository Root:", "Repository UUID:", "Revision:"];
    output
        .lines()
        .filter_map(|line| {
            supported
                .iter()
                .find(|prefix| line.starts_with(**prefix))
                .map(|_| line.replace('\\', "/"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_log_oneline(output: &str) -> String {
    output
        .lines()
        .filter_map(|line| {
            let parts = line.split(" | ").collect::<Vec<_>>();
            let revision = parts.first()?.trim();
            if !revision.starts_with('r') {
                return None;
            }
            let subject = if parts.len() >= 4 {
                parts[2].trim()
            } else {
                parts.get(1)?.trim()
            };
            Some(format!("{revision} | {subject}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_oneline_show_commit_log(output: &str) -> String {
    output
        .lines()
        .filter_map(|line| {
            let parts = line.split(" | ").collect::<Vec<_>>();
            if parts.len() < 3 {
                return None;
            }
            let revision = parts.first()?.trim();
            if !revision.starts_with('r') {
                return None;
            }
            let commit = parts.get(1)?.trim();
            let subject = parts.get(2)?.trim();
            Some(format!("{revision} | {commit} | {subject}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_rebase_dry_run(output: &str) -> String {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                None
            } else if line.contains("fetch") {
                Some("would fetch".to_string())
            } else if line.contains("rebase") {
                Some("would rebase <ref>".to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_incremental_log(output: &str) -> String {
    output
        .lines()
        .filter_map(|line| {
            if let Some(header) = normalize_log_header(line) {
                Some(header)
            } else if line.is_empty() || line == "Changed paths:" {
                None
            } else {
                Some(format!("message {line}"))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_verbose_log(output: &str) -> String {
    output
        .lines()
        .filter_map(|line| {
            if let Some(header) = normalize_log_header(line) {
                Some(header)
            } else if line.is_empty()
                || line == "Changed paths:"
                || line
                    == "------------------------------------------------------------------------"
            {
                None
            } else if let Some((action, path)) = parse_changed_path_line(line) {
                Some(format!("path {action} {path}"))
            } else {
                Some(format!("message {line}"))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_log_header(line: &str) -> Option<String> {
    let parts = line.split(" | ").map(str::trim).collect::<Vec<_>>();
    if parts.len() != 4
        || !parts[0]
            .strip_prefix('r')
            .is_some_and(|revision| revision.chars().all(|ch| ch.is_ascii_digit()))
        || !parts[3].ends_with(" line") && !parts[3].ends_with(" lines")
    {
        return None;
    }
    Some(format!(
        "revision {}\nauthor {}\ndate {}\ncount {}",
        parts[0], parts[1], parts[2], parts[3]
    ))
}

fn parse_changed_path_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    let (action, path) = if let Some((action, path)) = trimmed.split_once('\t') {
        (action.trim(), path.trim())
    } else {
        let mut parts = trimmed.split_whitespace();
        (parts.next()?, parts.next()?)
    };
    if !matches!(action.chars().next(), Some('A' | 'D' | 'M' | 'R' | 'C')) {
        return None;
    }
    Some((action.to_string(), path.to_string()))
}

fn normalize_find_rev_output(revision: u32, output: &str) -> String {
    if output.trim().is_empty() {
        return format!("r{revision} -> ");
    }
    format!("r{revision} -> {}", output.trim())
}

fn supported_file_modes(work_tree: &Path, refname: &str) -> Result<Vec<FileModeArtifact>, String> {
    Ok(run_text(
        work_tree,
        "git",
        &["ls-tree", "-r", "--format=%(objectmode) %(path)", refname],
    )?
    .lines()
    .filter_map(|line| {
        let (mode, path) = line.split_once(' ')?;
        Some(FileModeArtifact {
            mode: mode.to_string(),
            path: path.to_string(),
        })
    })
    .collect())
}

fn supported_tree_contents(
    work_tree: &Path,
    refname: &str,
    file_modes: &[FileModeArtifact],
) -> Result<Vec<String>, String> {
    file_modes
        .iter()
        .filter(|record| record.mode != "040000")
        .map(|record| {
            let spec = format!("{refname}:{}", record.path);
            let contents = run_text(work_tree, "git", &["show", &spec])?;
            Ok(format!(
                "{}\t{}",
                record.path,
                escape_artifact_value(&contents)
            ))
        })
        .collect()
}

fn supported_file_properties(
    work_tree: &Path,
    info_url: &str,
    file_modes: &[FileModeArtifact],
    empty_dir_placeholders: &[String],
) -> Result<Vec<FilePropertyArtifact>, String> {
    let mut records = Vec::new();
    let root_url = info_url.trim();
    for file in file_modes
        .iter()
        .filter(|record| record.mode != "040000" && !empty_dir_placeholders.contains(&record.path))
    {
        let url = format!(
            "{}/{}",
            root_url.trim_end_matches('/'),
            file.path.trim_start_matches('/')
        );
        for name in [
            "svn:executable",
            "svn:special",
            "svn:eol-style",
            "svn:mime-type",
            "svn:keywords",
            "svn:needs-lock",
        ] {
            match run_text(work_tree, "svn", &["propget", "--strict", name, &url]) {
                Ok(value) if !value.is_empty() => records.push(FilePropertyArtifact {
                    path: file.path.clone(),
                    name: name.to_string(),
                    value: normalize_property_value(name, &value),
                }),
                Ok(_) => {}
                Err(error) if error.contains(&format!("Property '{name}' not found")) => {}
                Err(error) => return Err(error),
            }
        }
    }
    records.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.value.cmp(&right.value))
    });
    Ok(records)
}

fn normalize_property_value(name: &str, value: &str) -> String {
    if matches!(name, "svn:executable" | "svn:special" | "svn:needs-lock") {
        "*".to_string()
    } else {
        value.to_string()
    }
}

fn escape_artifact_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn command_version(program: &str, args: &[&str]) -> String {
    match Command::new(program).args(args).output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        Ok(output) => String::from_utf8_lossy(&output.stderr).trim().to_string(),
        Err(error) => format!("unavailable: {error}"),
    }
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped
}

fn supported_empty_dir_placeholders(
    work_tree: &Path,
    refname: &str,
) -> Result<Vec<String>, String> {
    Ok(
        run_text(work_tree, "git", &["ls-tree", "-r", "--name-only", refname])?
            .lines()
            .filter(|line| line.ends_with("/.gitkeep"))
            .map(str::to_string)
            .collect(),
    )
}

fn supported_config(work_tree: &Path) -> Result<Vec<(String, String)>, String> {
    let required_keys = ["svn-remote.svn.url", "svn-remote.svn.fetch"];
    let optional_keys = [
        "svn-remote.svn.uuid",
        "svn-remote.svn.branches",
        "svn-remote.svn.tags",
        "svn-remote.svn.ignore-paths",
        "svn-remote.svn.include-paths",
        "svn-remote.svn.ignore-refs",
        "svn-remote.svn.authors-file",
        "svn-remote.svn.authors-prog",
        "svn-remote.svn.log-window-size",
        "svn-remote.svn.localtime",
        "svn-remote.svn.username",
        "svn-remote.svn.config-dir",
        "svn-remote.svn.no-auth-cache",
        "svn-remote.svn.noMetadata",
        "svn-remote.svn.rewriteRoot",
        "svn-remote.svn.rewriteUUID",
        "svn-remote.svn.preserve-empty-dirs",
        "svn-remote.svn.placeholder-filename",
    ];
    let mut config = Vec::new();
    for key in required_keys {
        let values = run_text(work_tree, "git", &["config", "--get-all", key])?;
        config.extend(values.lines().map(|value| {
            (
                key.to_string(),
                normalize_config_value(key, value).to_string(),
            )
        }));
    }
    for key in optional_keys {
        let output = Command::new("git")
            .current_dir(work_tree)
            .args(["config", "--get-all", key])
            .output()
            .map_err(|e| format!("git failed to start: {e}"))?;
        config.extend(optional_config_values(key, output)?);
    }
    config.sort();
    Ok(config)
}

fn optional_config_values(
    key: &str,
    output: std::process::Output,
) -> Result<Vec<(String, String)>, String> {
    if !output.status.success() {
        if output.status.code() == Some(1) && output.stdout.is_empty() && output.stderr.is_empty() {
            return Ok(Vec::new());
        }
        return Err(command_error("git", output));
    }
    let values = String::from_utf8_lossy(&output.stdout);
    Ok(values
        .lines()
        .map(|value| {
            (
                key.to_string(),
                normalize_config_value(key, value).to_string(),
            )
        })
        .collect())
}

fn normalize_config_value<'a>(key: &str, value: &'a str) -> &'a str {
    if matches!(
        key,
        "svn-remote.svn.fetch" | "svn-remote.svn.branches" | "svn-remote.svn.tags"
    ) {
        value.strip_prefix('+').unwrap_or(value)
    } else {
        value
    }
}

pub(crate) fn supported_rev_map(
    work_tree: &Path,
    refs: &[String],
) -> Result<Vec<RevMapArtifactRecord>, String> {
    let svn_dir = work_tree.join(".git").join("svn");
    let mut paths = Vec::new();
    collect_rev_map_paths(&svn_dir, &mut paths)?;
    paths.sort();
    if paths.is_empty() {
        return Err(format!("missing .rev_map under {}", svn_dir.display()));
    }

    let mut records = Vec::new();
    for path in paths {
        let source_ref = rev_map_source_ref(&svn_dir, &path, refs)?;
        let uuid = rev_map_uuid(&path)?;
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        let record_size = rev_map_record_size(work_tree, bytes.len()).ok_or_else(|| {
            format!(
                "unsupported .rev_map size for {}: {}",
                path.display(),
                bytes.len()
            )
        })?;
        if bytes.len() % record_size != 0 {
            return Err(format!(
                "unsupported .rev_map size for {}: {}",
                path.display(),
                bytes.len()
            ));
        }

        records.extend(bytes.chunks_exact(record_size).map(|record| {
            RevMapArtifactRecord {
                source_ref: source_ref.clone(),
                uuid: uuid.clone(),
                revision: u32::from_be_bytes([record[0], record[1], record[2], record[3]]),
                object_id: record[4..]
                    .iter()
                    .any(|byte| *byte != 0)
                    .then(|| hex::encode(&record[4..])),
            }
        }));
    }
    records.sort_by(|left, right| {
        left.source_ref
            .cmp(&right.source_ref)
            .then(left.uuid.cmp(&right.uuid))
            .then(left.revision.cmp(&right.revision))
    });
    Ok(records)
}

fn rev_map_record_size(work_tree: &Path, byte_len: usize) -> Option<usize> {
    match git_object_format(work_tree).as_deref() {
        Ok("sha1") if byte_len.is_multiple_of(24) => return Some(24),
        Ok("sha256") if byte_len.is_multiple_of(36) => return Some(36),
        _ => {}
    }

    if byte_len.is_multiple_of(24) && !byte_len.is_multiple_of(36) {
        Some(24)
    } else if byte_len.is_multiple_of(36) && !byte_len.is_multiple_of(24) {
        Some(36)
    } else if byte_len.is_multiple_of(24) {
        Some(24)
    } else {
        None
    }
}

fn git_object_format(work_tree: &Path) -> Result<String, String> {
    run_text(work_tree, "git", &["rev-parse", "--show-object-format"])
        .map(|format| format.trim().to_string())
}

pub(crate) fn supported_rev_map_byte_lengths(
    work_tree: &Path,
    refs: &[String],
) -> Result<Vec<RevMapByteLengthArtifact>, String> {
    let svn_dir = work_tree.join(".git").join("svn");
    let mut paths = Vec::new();
    collect_rev_map_paths(&svn_dir, &mut paths)?;
    paths.sort();
    if paths.is_empty() {
        return Err(format!("missing .rev_map under {}", svn_dir.display()));
    }

    let mut lengths = Vec::new();
    for path in paths {
        lengths.push(RevMapByteLengthArtifact {
            source_ref: rev_map_source_ref(&svn_dir, &path, refs)?,
            uuid: rev_map_uuid(&path)?,
            byte_len: fs::metadata(&path).map_err(|e| e.to_string())?.len() as usize,
        });
    }
    lengths.sort_by(|left, right| {
        left.source_ref
            .cmp(&right.source_ref)
            .then(left.uuid.cmp(&right.uuid))
    });
    Ok(lengths)
}

fn rev_map_uuid(rev_map: &Path) -> Result<String, String> {
    rev_map
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix(".rev_map."))
        .filter(|uuid| !uuid.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("unsupported rev-map filename: {}", rev_map.display()))
}

fn rev_map_source_ref(svn_dir: &Path, rev_map: &Path, refs: &[String]) -> Result<String, String> {
    let metadata_dir = rev_map
        .parent()
        .ok_or_else(|| format!("rev-map has no metadata parent: {}", rev_map.display()))?;
    let matches = refs
        .iter()
        .filter(|source_ref| {
            let perl_dir = svn_dir.join(source_ref);
            let rust_dir = source_ref
                .strip_prefix("refs/remotes/")
                .map(|short_ref| svn_dir.join(short_ref.replace('/', ".")));
            metadata_dir == perl_dir || rust_dir.as_deref() == Some(metadata_dir)
        })
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [source_ref] => Ok((*source_ref).clone()),
        [] => Err(format!(
            "rev-map {} does not match any known ref: {:?}",
            rev_map.display(),
            refs
        )),
        _ => Err(format!(
            "rev-map {} matches multiple known refs: {:?}",
            rev_map.display(),
            matches
        )),
    }
}

fn collect_rev_map_paths(path: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_rev_map_paths(&path, paths)?;
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".rev_map.") && !name.ends_with(".lock"))
        {
            paths.push(path);
        }
    }
    Ok(())
}

fn copy_dir_all(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_all(&source_path, &destination_path)?;
        } else {
            fs::copy(&source_path, &destination_path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn default_shared_fetch_args() -> SharedFetchArgs {
    SharedFetchArgs {
        authors_file: None,
        authors_prog: None,
        ignore_paths: None,
        include_paths: None,
        ignore_refs: None,
        revision: None,
        log_window_size: None,
        localtime: false,
        no_metadata: false,
        rewrite_root: None,
        rewrite_uuid: None,
        username: None,
        password: None,
        config_dir: None,
        no_auth_cache: false,
        preserve_empty_dirs: true,
        placeholder_filename: ".gitkeep".to_string(),
    }
}

fn format_config(config: &[(String, String)]) -> String {
    config
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_ref_tips(refs: &[RefTipArtifact]) -> String {
    refs.iter()
        .map(|artifact| format!("{} {}", artifact.name, artifact.object_id))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_clone_state(state: &CloneStateArtifact) -> String {
    format!(
        "head-symbolic {:?}\nhead-oid {:?}\nbranches\n{}\nindex\n{}\nworktree\n{}\nstatus\n{}",
        state.head_symbolic_ref,
        state.head_object_id,
        state.local_branches.join("\n"),
        state.index_entries.join("\n"),
        state.worktree_entries.join("\n"),
        state.status_porcelain_v2,
    )
}

fn format_commit_graph(commits: &[CommitGraphArtifact]) -> String {
    commits
        .iter()
        .map(|commit| {
            format!(
                "commit {}\nparents {}\ntree {}\nauthor {:?} {:?} {} {}\ncommitter {:?} {:?} {} {}\nmessage {:?}",
                commit.object_id,
                commit.parents.join(" "),
                commit.tree_id,
                commit.author_name,
                commit.author_email,
                commit.author_epoch,
                commit.author_offset,
                commit.committer_name,
                commit.committer_email,
                commit.committer_epoch,
                commit.committer_offset,
                commit.message,
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn format_rev_map(records: &[RevMapArtifactRecord]) -> String {
    records
        .iter()
        .map(|record| {
            format!(
                "{} {} {} {}",
                record.source_ref,
                record.uuid,
                record.revision,
                record.object_id.as_deref().unwrap_or("zero")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_rev_map_byte_lengths(records: &[RevMapByteLengthArtifact]) -> String {
    records
        .iter()
        .map(|record| format!("{} {} {}", record.source_ref, record.uuid, record.byte_len))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_file_modes(records: &[FileModeArtifact]) -> String {
    records
        .iter()
        .map(|record| format!("{} {}", record.mode, record.path))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_file_properties(records: &[FilePropertyArtifact]) -> String {
    records
        .iter()
        .map(|record| {
            format!(
                "{} {} {}",
                record.path,
                record.name,
                escape_artifact_value(&record.value)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

struct CapturedCommandOutput {
    status: i32,
    stdout: String,
    stderr: String,
}

fn normalize_clone_output(output: &CapturedCommandOutput) -> String {
    format!(
        "status: {}\nstdout:\n{}\nstderr:\n{}",
        output.status, output.stdout, output.stderr
    )
    .replace("perl-clone", "<clone>")
    .replace("rust-clone", "<clone>")
}

fn run(cwd: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(program)
        .current_dir(cwd)
        .args(args)
        .output()
        .map_err(|e| format!("{program} failed to start: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(program, output))
    }
}

fn run_capture(cwd: &Path, program: &str, args: &[&str]) -> Result<CapturedCommandOutput, String> {
    let output = Command::new(program)
        .current_dir(cwd)
        .args(args)
        .output()
        .map_err(|e| format!("{program} failed to start: {e}"))?;

    if output.status.success() {
        Ok(CapturedCommandOutput {
            status: 0,
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    } else {
        Err(command_error(program, output))
    }
}

#[cfg(unix)]
fn run_capture_with_stdin(
    cwd: &Path,
    program: &str,
    args: &[&str],
    stdin: &str,
) -> Result<CapturedCommandOutput, String> {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new(program)
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("{program} failed to start: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| format!("{program} stdin was not piped"))?
        .write_all(stdin.as_bytes())
        .map_err(|error| format!("failed to write {program} stdin: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("{program} failed while waiting: {error}"))?;
    if output.status.success() {
        Ok(CapturedCommandOutput {
            status: 0,
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    } else {
        Err(command_error(program, output))
    }
}

#[cfg(unix)]
fn run_capture_expect_failure(
    cwd: &Path,
    program: &str,
    args: &[&str],
) -> Result<CapturedCommandOutput, String> {
    let output = Command::new(program)
        .current_dir(cwd)
        .args(args)
        .output()
        .map_err(|error| format!("{program} failed to start: {error}"))?;
    if output.status.success() {
        return Err(format!("{program} unexpectedly succeeded"));
    }
    Ok(CapturedCommandOutput {
        status: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn run_text(cwd: &Path, program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .current_dir(cwd)
        .args(args)
        .output()
        .map_err(|e| format!("{program} failed to start: {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(command_error(program, output))
    }
}

fn command_error(program: &str, output: std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    format!(
        "{program} failed with status {}: {}{}",
        output.status,
        stderr.trim(),
        if stdout.trim().is_empty() {
            String::new()
        } else {
            format!(" stdout: {}", stdout.trim())
        }
    )
}

fn path_arg(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()))
}

fn file_url(path: &Path) -> Result<String, String> {
    let raw = path
        .canonicalize()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    let raw = raw.strip_prefix("//?/").unwrap_or(&raw);
    Ok(format!("file:///{}", raw.trim_start_matches('/')))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Output;

    #[test]
    fn rejects_git_svn_rs_shim_as_perl_git_svn() {
        assert_eq!(
            classify_git_svn_version("git-svn-rs 0.1.0"),
            ToolAvailability::Missing {
                reason: "git svn resolved to git-svn-rs shim, not Perl git-svn: git-svn-rs 0.1.0"
                    .to_string()
            }
        );
    }

    #[test]
    fn accepts_frozen_perl_git_svn_version() {
        assert_eq!(
            classify_git_svn_version("git-svn version 2.54.0 (svn 1.14.3)"),
            ToolAvailability::Available {
                version: "git-svn version 2.54.0 (svn 1.14.3)".to_string()
            }
        );
    }

    #[test]
    fn rejects_non_frozen_perl_git_svn_version() {
        assert_eq!(
            classify_git_svn_version("git-svn version 2.43.0 (svn 1.14.3)"),
            ToolAvailability::Missing {
                reason: "frozen Perl git-svn 2.54.0 is required, detected: git-svn version 2.43.0 (svn 1.14.3)".to_string()
            }
        );
    }

    #[test]
    fn iso8601_offsets_normalize_utc_and_numeric_offsets() {
        assert_eq!(iso8601_offset("2026-07-27T03:32:03Z").unwrap(), "+0000");
        assert_eq!(
            iso8601_offset("2026-01-02T03:04:05+05:30").unwrap(),
            "+0530"
        );
    }

    #[test]
    fn structured_log_normalizers_retain_identity_date_count_and_message() {
        let header = "r2 | alice | 2026-01-01 08:00:00 +0800 (Thu, 01 Jan 2026) | 2 lines";
        let incremental = format!("{header}\n\nsubject\nbody\n");
        let verbose = format!(
            "------------------------------------------------------------------------\n{header}\nChanged paths:\n   M src/lib.rs\n\nsubject\nbody\n------------------------------------------------------------------------\n"
        );
        let expected_header = "revision r2\nauthor alice\ndate 2026-01-01 08:00:00 +0800 (Thu, 01 Jan 2026)\ncount 2 lines";

        assert_eq!(
            normalize_incremental_log(&incremental),
            format!("{expected_header}\nmessage subject\nmessage body")
        );
        assert_eq!(
            normalize_verbose_log(&verbose),
            format!("{expected_header}\npath M src/lib.rs\nmessage subject\nmessage body")
        );
    }

    #[test]
    fn supported_rev_map_normalizes_perl_and_rust_paths_without_flattening_revisions() {
        let tmp = tempfile::tempdir().unwrap();
        let svn_dir = tmp.path().join(".git/svn");
        write_rev_map_fixture(&svn_dir.join("refs/remotes/origin/trunk/.rev_map.uuid"), 1);
        write_rev_map_fixture(&svn_dir.join("origin.branches.main/.rev_map.uuid"), 1);
        let refs = vec![
            "refs/remotes/origin/trunk".to_string(),
            "refs/remotes/origin/branches/main".to_string(),
        ];

        let records = supported_rev_map(tmp.path(), &refs).unwrap();

        assert_eq!(
            records,
            vec![
                RevMapArtifactRecord {
                    source_ref: "refs/remotes/origin/branches/main".to_string(),
                    uuid: "uuid".to_string(),
                    revision: 1,
                    object_id: Some("01".repeat(20)),
                },
                RevMapArtifactRecord {
                    source_ref: "refs/remotes/origin/trunk".to_string(),
                    uuid: "uuid".to_string(),
                    revision: 1,
                    object_id: Some("01".repeat(20)),
                },
            ]
        );
    }

    #[test]
    fn exact_ref_graph_and_clone_state_collectors_preserve_identity() {
        let tmp = tempfile::tempdir().unwrap();
        run(tmp.path(), "git", &["init"]).unwrap();
        fs::write(tmp.path().join("file.txt"), b"content\n").unwrap();
        run(tmp.path(), "git", &["add", "file.txt"]).unwrap();
        run(
            tmp.path(),
            "git",
            &[
                "-c",
                "user.name=Artifact Author",
                "-c",
                "user.email=artifact@example.com",
                "commit",
                "--date=2026-01-02T03:04:05+05:30",
                "-m",
                "artifact message",
            ],
        )
        .unwrap();
        let head = run_text(tmp.path(), "git", &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        run(
            tmp.path(),
            "git",
            &["update-ref", "refs/remotes/origin/trunk", &head],
        )
        .unwrap();

        let refs = supported_ref_tips(tmp.path()).unwrap();
        assert_eq!(
            refs,
            vec![RefTipArtifact {
                name: "refs/remotes/origin/trunk".to_string(),
                object_id: head.clone(),
            }]
        );
        let graph =
            supported_commit_graph(tmp.path(), &["refs/remotes/origin/trunk".to_string()]).unwrap();
        assert_eq!(graph.len(), 1);
        assert_eq!(graph[0].object_id, head);
        assert!(graph[0].parents.is_empty());
        assert_eq!(graph[0].author_name, "Artifact Author");
        assert_eq!(graph[0].author_email, "artifact@example.com");
        assert_eq!(graph[0].author_offset, "+0530");
        assert!(graph[0].message.contains("artifact message"));

        let state = collect_clone_state(tmp.path()).unwrap();
        assert!(
            state
                .head_symbolic_ref
                .as_deref()
                .is_some_and(|head| head.starts_with("refs/heads/"))
        );
        assert_eq!(
            state.head_object_id.as_deref(),
            Some(graph[0].object_id.as_str())
        );
        assert_eq!(state.index_entries.len(), 1);
        assert_eq!(state.worktree_entries, vec!["file.txt\t636f6e74656e740a"]);

        let no_checkout = tempfile::tempdir().unwrap();
        let no_checkout_path = no_checkout.path().join("clone");
        run(
            no_checkout.path(),
            "git",
            &[
                "clone",
                "--no-checkout",
                path_arg(tmp.path()).unwrap(),
                path_arg(&no_checkout_path).unwrap(),
            ],
        )
        .unwrap();
        let no_checkout_state = collect_clone_state(&no_checkout_path).unwrap();
        assert!(no_checkout_state.head_object_id.is_some());
        assert!(no_checkout_state.index_entries.is_empty());
        assert!(no_checkout_state.worktree_entries.is_empty());
    }

    #[test]
    fn supported_rev_map_preserves_zero_records() {
        let tmp = tempfile::tempdir().unwrap();
        let rev_map = tmp
            .path()
            .join(".git/svn/refs/remotes/origin/trunk/.rev_map.uuid");
        write_zero_rev_map_fixture(&rev_map, 3);
        let refs = vec!["refs/remotes/origin/trunk".to_string()];

        let records = supported_rev_map(tmp.path(), &refs).unwrap();

        assert_eq!(
            records,
            vec![RevMapArtifactRecord {
                source_ref: "refs/remotes/origin/trunk".to_string(),
                uuid: "uuid".to_string(),
                revision: 3,
                object_id: None,
            }]
        );
    }

    #[test]
    fn supported_rev_map_reads_sha256_records() {
        let tmp = tempfile::tempdir().unwrap();
        let rev_map = tmp
            .path()
            .join(".git/svn/refs/remotes/git-svn/.rev_map.uuid");
        write_sha256_rev_map_fixture(&rev_map, 5);
        let refs = vec!["refs/remotes/git-svn".to_string()];

        let records = supported_rev_map(tmp.path(), &refs).unwrap();

        assert_eq!(
            records,
            vec![RevMapArtifactRecord {
                source_ref: "refs/remotes/git-svn".to_string(),
                uuid: "uuid".to_string(),
                revision: 5,
                object_id: Some("05".repeat(32)),
            }]
        );
    }

    #[test]
    fn supported_rev_map_artifacts_include_rev_map_uuid() {
        let tmp = tempfile::tempdir().unwrap();
        let rev_map = tmp
            .path()
            .join(".git/svn/refs/remotes/origin/trunk/.rev_map.repository-uuid");
        write_rev_map_fixture(&rev_map, 7);
        let refs = vec!["refs/remotes/origin/trunk".to_string()];

        let records = supported_rev_map(tmp.path(), &refs).unwrap();
        let lengths = supported_rev_map_byte_lengths(tmp.path(), &refs).unwrap();

        assert_eq!(
            records,
            vec![RevMapArtifactRecord {
                source_ref: "refs/remotes/origin/trunk".to_string(),
                uuid: "repository-uuid".to_string(),
                revision: 7,
                object_id: Some("07".repeat(20)),
            }]
        );
        assert_eq!(
            lengths,
            vec![RevMapByteLengthArtifact {
                source_ref: "refs/remotes/origin/trunk".to_string(),
                uuid: "repository-uuid".to_string(),
                byte_len: 24,
            }]
        );
        assert_eq!(
            format_rev_map(&records),
            format!(
                "refs/remotes/origin/trunk repository-uuid 7 {}",
                "07".repeat(20)
            )
        );
        assert_eq!(
            format_rev_map_byte_lengths(&lengths),
            "refs/remotes/origin/trunk repository-uuid 24"
        );
    }

    #[test]
    fn format_rev_map_includes_source_ref() {
        let records = vec![RevMapArtifactRecord {
            source_ref: "refs/remotes/origin/trunk".to_string(),
            uuid: "uuid".to_string(),
            revision: 3,
            object_id: None,
        }];

        assert_eq!(
            format_rev_map(&records),
            "refs/remotes/origin/trunk uuid 3 zero"
        );
    }

    #[test]
    fn supported_rev_map_rejects_unmatched_metadata_path() {
        let tmp = tempfile::tempdir().unwrap();
        let rev_map = tmp.path().join(".git/svn/unknown/.rev_map.uuid");
        write_rev_map_fixture(&rev_map, 1);
        let refs = vec!["refs/remotes/origin/trunk".to_string()];

        let error = supported_rev_map(tmp.path(), &refs).unwrap_err();

        assert!(error.contains("does not match any known ref"), "{error}");
    }

    #[test]
    fn supported_rev_map_rejects_ambiguous_rust_metadata_path() {
        let tmp = tempfile::tempdir().unwrap();
        let rev_map = tmp.path().join(".git/svn/origin.branch.main/.rev_map.uuid");
        write_rev_map_fixture(&rev_map, 1);
        let refs = vec![
            "refs/remotes/origin/branch/main".to_string(),
            "refs/remotes/origin.branch/main".to_string(),
        ];

        let error = supported_rev_map(tmp.path(), &refs).unwrap_err();

        assert!(error.contains("matches multiple known refs"), "{error}");
        assert!(error.contains(&refs[0]), "{error}");
        assert!(error.contains(&refs[1]), "{error}");
    }

    fn write_rev_map_fixture(path: &Path, revision: u32) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut bytes = revision.to_be_bytes().to_vec();
        bytes.extend([revision as u8; 20]);
        fs::write(path, bytes).unwrap();
    }

    fn write_zero_rev_map_fixture(path: &Path, revision: u32) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut bytes = revision.to_be_bytes().to_vec();
        bytes.extend([0; 20]);
        fs::write(path, bytes).unwrap();
    }

    fn write_sha256_rev_map_fixture(path: &Path, revision: u32) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut bytes = revision.to_be_bytes().to_vec();
        bytes.extend([revision as u8; 32]);
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn supported_config_includes_svn_remote_uuid() {
        let tmp = tempfile::tempdir().unwrap();
        run(tmp.path(), "git", &["init"]).unwrap();
        run(
            tmp.path(),
            "git",
            &["config", "svn-remote.svn.url", "file:///repo"],
        )
        .unwrap();
        run(
            tmp.path(),
            "git",
            &[
                "config",
                "svn-remote.svn.fetch",
                "+trunk:refs/remotes/origin/trunk",
            ],
        )
        .unwrap();
        run(
            tmp.path(),
            "git",
            &["config", "svn-remote.svn.uuid", "fixture-uuid"],
        )
        .unwrap();

        let config = supported_config(tmp.path()).unwrap();

        assert!(config.contains(&(
            "svn-remote.svn.uuid".to_string(),
            "fixture-uuid".to_string()
        )));
    }

    #[test]
    fn supported_config_accepts_absent_svn_remote_uuid() {
        let tmp = tempfile::tempdir().unwrap();
        run(tmp.path(), "git", &["init"]).unwrap();
        run(
            tmp.path(),
            "git",
            &["config", "svn-remote.svn.url", "file:///repo"],
        )
        .unwrap();
        run(
            tmp.path(),
            "git",
            &[
                "config",
                "svn-remote.svn.fetch",
                "+trunk:refs/remotes/origin/trunk",
            ],
        )
        .unwrap();

        assert_eq!(
            supported_config(tmp.path()).unwrap(),
            vec![
                (
                    "svn-remote.svn.fetch".to_string(),
                    "trunk:refs/remotes/origin/trunk".to_string()
                ),
                ("svn-remote.svn.url".to_string(), "file:///repo".to_string()),
            ]
        );
    }

    #[test]
    fn supported_config_includes_optional_branch_and_tag_mappings() {
        let tmp = tempfile::tempdir().unwrap();
        run(tmp.path(), "git", &["init"]).unwrap();
        run(
            tmp.path(),
            "git",
            &["config", "svn-remote.svn.url", "file:///repo"],
        )
        .unwrap();
        run(
            tmp.path(),
            "git",
            &[
                "config",
                "svn-remote.svn.fetch",
                "+trunk:refs/remotes/origin/trunk",
            ],
        )
        .unwrap();
        run(
            tmp.path(),
            "git",
            &["config", "svn-remote.svn.uuid", "fixture-uuid"],
        )
        .unwrap();
        run(
            tmp.path(),
            "git",
            &[
                "config",
                "--add",
                "svn-remote.svn.branches",
                "+branches/*:refs/remotes/origin/*",
            ],
        )
        .unwrap();
        run(
            tmp.path(),
            "git",
            &[
                "config",
                "--add",
                "svn-remote.svn.tags",
                "+tags/*:refs/remotes/origin/tags/*",
            ],
        )
        .unwrap();

        let config = supported_config(tmp.path()).unwrap();

        assert_eq!(
            config,
            vec![
                (
                    "svn-remote.svn.branches".to_string(),
                    "branches/*:refs/remotes/origin/*".to_string()
                ),
                (
                    "svn-remote.svn.fetch".to_string(),
                    "trunk:refs/remotes/origin/trunk".to_string()
                ),
                (
                    "svn-remote.svn.tags".to_string(),
                    "tags/*:refs/remotes/origin/tags/*".to_string()
                ),
                ("svn-remote.svn.url".to_string(), "file:///repo".to_string()),
                (
                    "svn-remote.svn.uuid".to_string(),
                    "fixture-uuid".to_string()
                ),
            ]
        );
    }

    #[test]
    fn supported_config_includes_optional_metadata_keys() {
        let tmp = tempfile::tempdir().unwrap();
        run(tmp.path(), "git", &["init"]).unwrap();
        let entries = [
            ("svn-remote.svn.url", "file:///repo"),
            ("svn-remote.svn.fetch", "+trunk:refs/remotes/origin/trunk"),
            ("svn-remote.svn.uuid", "fixture-uuid"),
            ("svn-remote.svn.ignore-paths", "^docs/"),
            ("svn-remote.svn.include-paths", "^trunk/"),
            ("svn-remote.svn.ignore-refs", "^refs/remotes/origin/tmp"),
            ("svn-remote.svn.authors-file", "authors.txt"),
            ("svn-remote.svn.authors-prog", "authors-prog"),
            ("svn-remote.svn.log-window-size", "42"),
            ("svn-remote.svn.localtime", "true"),
            ("svn-remote.svn.noMetadata", "true"),
            ("svn-remote.svn.rewriteRoot", "https://mirror.example/repo"),
            ("svn-remote.svn.rewriteUUID", "mirror-uuid"),
            ("svn-remote.svn.preserve-empty-dirs", "true"),
            ("svn-remote.svn.placeholder-filename", ".empty"),
        ];
        for (key, value) in entries {
            run(tmp.path(), "git", &["config", key, value]).unwrap();
        }

        let config = supported_config(tmp.path()).unwrap();

        for (key, value) in [
            ("svn-remote.svn.ignore-paths", "^docs/"),
            ("svn-remote.svn.include-paths", "^trunk/"),
            ("svn-remote.svn.ignore-refs", "^refs/remotes/origin/tmp"),
            ("svn-remote.svn.authors-file", "authors.txt"),
            ("svn-remote.svn.authors-prog", "authors-prog"),
            ("svn-remote.svn.log-window-size", "42"),
            ("svn-remote.svn.localtime", "true"),
            ("svn-remote.svn.noMetadata", "true"),
            ("svn-remote.svn.rewriteRoot", "https://mirror.example/repo"),
            ("svn-remote.svn.rewriteUUID", "mirror-uuid"),
            ("svn-remote.svn.preserve-empty-dirs", "true"),
            ("svn-remote.svn.placeholder-filename", ".empty"),
        ] {
            assert!(
                config.contains(&(key.to_string(), value.to_string())),
                "missing {key} in {config:?}"
            );
        }
    }

    #[test]
    fn supported_config_includes_optional_auth_keys_without_password() {
        let tmp = tempfile::tempdir().unwrap();
        run(tmp.path(), "git", &["init"]).unwrap();
        for (key, value) in [
            ("svn-remote.svn.url", "file:///repo"),
            ("svn-remote.svn.fetch", "+trunk:refs/remotes/origin/trunk"),
            ("svn-remote.svn.uuid", "fixture-uuid"),
            ("svn-remote.svn.username", "alice"),
            ("svn-remote.svn.password", "secret"),
            ("svn-remote.svn.config-dir", "svn-config"),
            ("svn-remote.svn.no-auth-cache", "true"),
        ] {
            run(tmp.path(), "git", &["config", key, value]).unwrap();
        }

        let config = supported_config(tmp.path()).unwrap();

        assert!(config.contains(&("svn-remote.svn.username".to_string(), "alice".to_string())));
        assert!(config.contains(&(
            "svn-remote.svn.config-dir".to_string(),
            "svn-config".to_string()
        )));
        assert!(config.contains(&(
            "svn-remote.svn.no-auth-cache".to_string(),
            "true".to_string()
        )));
        assert!(
            config
                .iter()
                .all(|(key, _)| key != "svn-remote.svn.password"),
            "password must not be captured in golden config artifacts: {config:?}"
        );
    }

    #[test]
    fn supported_config_strips_only_one_refspec_force_prefix() {
        assert_eq!(
            normalize_config_value("svn-remote.svn.fetch", "++trunk:refs/remotes/git-svn"),
            "+trunk:refs/remotes/git-svn"
        );
        assert_eq!(
            normalize_config_value(
                "svn-remote.svn.branches",
                "++branches/*:refs/remotes/origin/*"
            ),
            "+branches/*:refs/remotes/origin/*"
        );
        assert_eq!(
            normalize_config_value("svn-remote.svn.url", "++not-a-refspec"),
            "++not-a-refspec"
        );
    }

    #[test]
    fn optional_config_values_only_skips_missing_key_status() {
        let output = output_with_status(128, b"", b"fatal: bad config\n");

        let error = optional_config_values("svn-remote.svn.branches", output).unwrap_err();

        assert!(error.contains("fatal: bad config"), "{error}");
    }

    #[test]
    fn optional_config_values_treats_missing_key_as_absent() {
        let output = output_with_status(1, b"", b"");

        let values = optional_config_values("svn-remote.svn.branches", output).unwrap();

        assert!(values.is_empty());
    }

    #[test]
    fn optional_config_values_reports_status_one_with_stderr() {
        let output = output_with_status(1, b"", b"warning: config issue\n");

        let error = optional_config_values("svn-remote.svn.branches", output).unwrap_err();

        assert!(error.contains("warning: config issue"), "{error}");
    }

    #[cfg(windows)]
    fn output_with_status(code: u32, stdout: &[u8], stderr: &[u8]) -> Output {
        use std::os::windows::process::ExitStatusExt;

        Output {
            status: std::process::ExitStatus::from_raw(code),
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        }
    }

    #[cfg(unix)]
    fn output_with_status(code: i32, stdout: &[u8], stderr: &[u8]) -> Output {
        use std::os::unix::process::ExitStatusExt;

        Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        }
    }
}
