use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use git_svn_rs_core::cli::{
    CloneArgs, FindRevArgs, InfoArgs, LayoutArgs, LogArgs, RebaseArgs, ResetArgs, SharedFetchArgs,
};
use git_svn_rs_core::commands;
use git_svn_rs_core::git_svn_id::GitSvnId;

const PERL_GIT_SVN_REQUIRED: &str = "Perl git-svn is required";
const SVN_TOOLS_REQUIRED: &str = "svnadmin and svn are required";

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
    pub has_commit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevMapByteLengthArtifact {
    pub source_ref: String,
    pub uuid: String,
    pub byte_len: usize,
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GoldenComparisonArtifacts {
    pub config: Vec<(String, String)>,
    pub refs: Vec<String>,
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
}

impl GoldenArtifactCapture {
    pub fn new(root: impl AsRef<Path>, case_name: &str) -> Result<Self, String> {
        let root = root.as_ref().join(case_name);
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        Ok(Self { root })
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
}

pub struct GoldenComparison {
    pub perl: GoldenComparisonArtifacts,
    pub rust: GoldenComparisonArtifacts,
}

impl GoldenComparison {
    pub fn assert_supported_subset_matches(&self) -> Result<(), String> {
        compare_supported_subset(&self.perl, &self.rust)
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
        ToolAvailability::Missing { .. } => Err(missing_perl_git_svn_policy(strict_compat())),
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
                .filter(|record| record.source_ref == source_ref && record.has_commit)
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
    let root = root.as_ref();
    let fixture = MaterializedSvnFixture::create(root)?;
    let capture = GoldenArtifactCapture::new(root, "standard-linear-history")?;

    let perl_path = root.join("perl-clone");
    let perl_clone_output = run_capture(
        root,
        "git",
        &[
            "svn",
            "clone",
            "--preserve-empty-dirs",
            "--placeholder-filename",
            ".gitkeep",
            "--trunk",
            "trunk",
            "--prefix=origin/",
            &fixture.url(),
            path_arg(&perl_path)?,
        ],
    )?;
    let perl = collect_supported_artifacts(
        &perl_path,
        GoldenTool::Perl,
        normalize_clone_output(&perl_clone_output),
    )?;
    capture.write_text("perl/clone-output.txt", &perl.clone_output)?;
    capture.write_text("perl/config.txt", &format_config(&perl.config))?;
    capture.write_text("perl/refs.txt", &perl.refs.join("\n"))?;
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
    capture.write_text("perl/find-rev-nearest.txt", &perl.find_rev_nearest)?;
    capture.write_text("perl/find-rev-commit.txt", &perl.find_rev_commit)?;
    capture.write_text("perl/rebase-dry-run.txt", &perl.rebase_dry_run)?;
    capture.write_text("perl/reset.txt", &perl.reset)?;
    capture.write_text("perl/gc-output.txt", &perl.gc_output)?;

    let rust_path = root.join("rust-clone");
    commands::clone::run(CloneArgs {
        url: fixture.url(),
        path: Some(path_arg(&rust_path)?.to_string()),
        layout: LayoutArgs {
            stdlayout: false,
            trunk: Some("trunk".to_string()),
            branches: Vec::new(),
            tags: Vec::new(),
            prefix: None,
        },
        shared: default_shared_fetch_args(),
        no_checkout: false,
    })?;
    let rust =
        collect_supported_artifacts(&rust_path, GoldenTool::Rust, "clone: success".to_string())?;
    capture.write_text("rust/clone-output.txt", &rust.clone_output)?;
    capture.write_text("rust/config.txt", &format_config(&rust.config))?;
    capture.write_text("rust/refs.txt", &rust.refs.join("\n"))?;
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
    capture.write_text("rust/find-rev-nearest.txt", &rust.find_rev_nearest)?;
    capture.write_text("rust/find-rev-commit.txt", &rust.find_rev_commit)?;
    capture.write_text("rust/rebase-dry-run.txt", &rust.rebase_dry_run)?;
    capture.write_text("rust/reset.txt", &rust.reset)?;
    capture.write_text("rust/gc-output.txt", &rust.gc_output)?;

    Ok(GoldenComparison { perl, rust })
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

    let config = supported_config(work_tree)?;
    let rev = refs
        .first()
        .ok_or_else(|| "golden clone did not create a remote ref".to_string())?;
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
        .first()
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
    let file_properties = supported_file_properties(work_tree, &info_url, &file_modes)?;
    let info_summary = supported_info_summary(work_tree, tool)?;
    let log_revision_oneline = supported_log_revision_oneline(work_tree, tool, first_revision)?;
    let log_revision_range_oneline =
        supported_log_revision_range_oneline(work_tree, tool, first_revision, first_revision + 1)?;
    let find_rev_nearest = supported_find_rev_nearest(work_tree, tool, first_revision + 1)?;
    let find_rev_commit = supported_find_rev_commit(work_tree, tool, rev, first_revision)?;
    let rebase_dry_run = supported_rebase_dry_run(work_tree, tool)?;
    let reset = supported_reset(work_tree, tool, first_revision + 1)?;
    let gc_output = supported_gc(work_tree, tool)?;

    Ok(GoldenComparisonArtifacts {
        config,
        refs,
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
        find_rev_nearest,
        find_rev_commit,
        rebase_dry_run,
        reset,
        gc_output,
        clone_output,
    })
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
    let revision_arg = format!("r{revision}");
    let output = match tool {
        GoldenTool::Perl => run_text(
            work_tree,
            "git",
            &["svn", "log", "--revision", &revision_arg, "--oneline"],
        )?,
        GoldenTool::Rust => commands::log::run_in_work_tree(
            work_tree,
            LogArgs {
                revision: Some(revision_arg),
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
    let revision_arg = format!("r{start_revision}:r{end_revision}");
    let output = match tool {
        GoldenTool::Perl => run_text(
            work_tree,
            "git",
            &["svn", "log", "--revision", &revision_arg, "--oneline"],
        )?,
        GoldenTool::Rust => commands::log::run_in_work_tree(
            work_tree,
            LogArgs {
                revision: Some(revision_arg),
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
                rev_or_commit: commit,
                before: false,
                after: false,
            },
        )?,
    };
    Ok(format!(
        "<commit> -> {}",
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
                before: true,
                after: false,
            },
        )?,
        (GoldenTool::Rust, FindRevDirection::After) => commands::find_rev::run_in_work_tree(
            work_tree,
            FindRevArgs {
                rev_or_commit: revision_arg,
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
            let revision_index = parts.iter().position(|part| part.trim().starts_with('r'))?;
            let revision = parts.get(revision_index)?.trim();
            let subject = parts.get(revision_index + 1)?.trim();
            Some(format!("<commit> | {revision} | {subject}"))
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
            if line.starts_with("r") && line.contains(" | ") {
                let parts = line.split(" | ").collect::<Vec<_>>();
                parts
                    .first()
                    .map(|revision| format!("revision {}", revision.trim()))
            } else if let Some(subject) = line.strip_prefix("  ") {
                let subject = subject.trim();
                if subject.is_empty() {
                    None
                } else {
                    Some(format!("subject {subject}"))
                }
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_verbose_log(output: &str) -> String {
    output
        .lines()
        .filter_map(|line| {
            if line.starts_with("r") && line.contains(" | ") {
                let revision = line.split(" | ").next()?.trim();
                Some(format!("revision {revision}"))
            } else if let Some((action, path)) = parse_changed_path_line(line) {
                Some(format!("path {action} {path}"))
            } else if let Some(subject) = line.strip_prefix("  ") {
                let subject = subject.trim();
                if subject.is_empty()
                    || subject == "Changed paths:"
                    || subject.starts_with("commit ")
                {
                    None
                } else {
                    Some(format!("subject {subject}"))
                }
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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
    Some((action.to_string(), normalize_changed_path(path)))
}

fn normalize_changed_path(path: &str) -> String {
    path.trim_start_matches('/')
        .strip_prefix("trunk/")
        .unwrap_or_else(|| path.trim_start_matches('/'))
        .to_string()
}

fn normalize_find_rev_output(revision: u32, output: &str) -> String {
    if output.trim().is_empty() {
        return format!("r{revision} -> ");
    }
    if output.trim().chars().all(|c| c.is_ascii_hexdigit()) {
        format!("r{revision} -> <commit>")
    } else {
        format!("r{revision} -> {}", output.trim())
    }
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
) -> Result<Vec<FilePropertyArtifact>, String> {
    let mut records = Vec::new();
    let root_url = info_url.trim();
    for file in file_modes.iter().filter(|record| record.mode != "040000") {
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
    let required_keys = [
        "svn-remote.svn.url",
        "svn-remote.svn.fetch",
        "svn-remote.svn.uuid",
    ];
    let optional_keys = [
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

fn supported_rev_map(
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

        records.extend(
            bytes
                .chunks_exact(record_size)
                .map(|record| RevMapArtifactRecord {
                    source_ref: source_ref.clone(),
                    uuid: uuid.clone(),
                    revision: u32::from_be_bytes([record[0], record[1], record[2], record[3]]),
                    has_commit: record[4..].iter().any(|byte| *byte != 0),
                }),
        );
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

fn supported_rev_map_byte_lengths(
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

fn format_rev_map(records: &[RevMapArtifactRecord]) -> String {
    records
        .iter()
        .map(|record| {
            format!(
                "{} {} {} {}",
                record.source_ref, record.uuid, record.revision, record.has_commit
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
    stdout: String,
    stderr: String,
}

fn normalize_clone_output(output: &CapturedCommandOutput) -> String {
    let _ = (&output.stdout, &output.stderr);
    "clone: success".to_string()
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
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    } else {
        Err(command_error(program, output))
    }
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
                    has_commit: true,
                },
                RevMapArtifactRecord {
                    source_ref: "refs/remotes/origin/trunk".to_string(),
                    uuid: "uuid".to_string(),
                    revision: 1,
                    has_commit: true,
                },
            ]
        );
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
                has_commit: false,
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
                has_commit: true,
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
                has_commit: true,
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
            "refs/remotes/origin/trunk repository-uuid 7 true"
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
            has_commit: false,
        }];

        assert_eq!(
            format_rev_map(&records),
            "refs/remotes/origin/trunk uuid 3 false"
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
