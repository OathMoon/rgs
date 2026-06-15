use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use git_svn_rs_core::cli::{
    CloneArgs, FindRevArgs, InfoArgs, LayoutArgs, LogArgs, SharedFetchArgs,
};
use git_svn_rs_core::commands;

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
    pub revision: u32,
    pub has_commit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileModeArtifact {
    pub mode: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GoldenComparisonArtifacts {
    pub config: Vec<(String, String)>,
    pub refs: Vec<String>,
    pub git_svn_id_footers: Vec<String>,
    pub rev_map: Vec<RevMapArtifactRecord>,
    pub file_modes: Vec<FileModeArtifact>,
    pub log_oneline: String,
    pub find_rev: String,
    pub info_url: String,
    pub info_summary: String,
    pub log_revision_oneline: String,
    pub find_rev_nearest: String,
    pub find_rev_commit: String,
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

pub fn run_standard_trunk_golden_comparison(
    root: impl AsRef<Path>,
) -> Result<GoldenComparison, String> {
    let root = root.as_ref();
    let fixture = MaterializedSvnFixture::create(root)?;
    let capture = GoldenArtifactCapture::new(root, "standard-linear-history")?;

    let perl_path = root.join("perl-clone");
    run(
        root,
        "git",
        &[
            "svn",
            "clone",
            "--trunk",
            "trunk",
            "--prefix=origin/",
            &fixture.url(),
            path_arg(&perl_path)?,
        ],
    )?;
    let perl = collect_supported_artifacts(&perl_path, GoldenTool::Perl)?;
    capture.write_text("perl/config.txt", &format_config(&perl.config))?;
    capture.write_text("perl/refs.txt", &perl.refs.join("\n"))?;
    capture.write_text(
        "perl/git-svn-id-footers.txt",
        &perl.git_svn_id_footers.join("\n"),
    )?;
    capture.write_text("perl/rev-map.txt", &format_rev_map(&perl.rev_map))?;
    capture.write_text("perl/file-modes.txt", &format_file_modes(&perl.file_modes))?;
    capture.write_text("perl/log-oneline.txt", &perl.log_oneline)?;
    capture.write_text("perl/find-rev.txt", &perl.find_rev)?;
    capture.write_text("perl/info-url.txt", &perl.info_url)?;
    capture.write_text("perl/info-summary.txt", &perl.info_summary)?;
    capture.write_text("perl/log-revision-oneline.txt", &perl.log_revision_oneline)?;
    capture.write_text("perl/find-rev-nearest.txt", &perl.find_rev_nearest)?;
    capture.write_text("perl/find-rev-commit.txt", &perl.find_rev_commit)?;

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
    let rust = collect_supported_artifacts(&rust_path, GoldenTool::Rust)?;
    capture.write_text("rust/config.txt", &format_config(&rust.config))?;
    capture.write_text("rust/refs.txt", &rust.refs.join("\n"))?;
    capture.write_text(
        "rust/git-svn-id-footers.txt",
        &rust.git_svn_id_footers.join("\n"),
    )?;
    capture.write_text("rust/rev-map.txt", &format_rev_map(&rust.rev_map))?;
    capture.write_text("rust/file-modes.txt", &format_file_modes(&rust.file_modes))?;
    capture.write_text("rust/log-oneline.txt", &rust.log_oneline)?;
    capture.write_text("rust/find-rev.txt", &rust.find_rev)?;
    capture.write_text("rust/info-url.txt", &rust.info_url)?;
    capture.write_text("rust/info-summary.txt", &rust.info_summary)?;
    capture.write_text("rust/log-revision-oneline.txt", &rust.log_revision_oneline)?;
    capture.write_text("rust/find-rev-nearest.txt", &rust.find_rev_nearest)?;
    capture.write_text("rust/find-rev-commit.txt", &rust.find_rev_commit)?;

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
    if perl.file_modes != rust.file_modes {
        mismatches.push(format!(
            "file modes differ\nperl: {:?}\nrust: {:?}",
            perl.file_modes, rust.file_modes
        ));
    }
    if perl.log_oneline != rust.log_oneline {
        mismatches.push(format!(
            "log --oneline differs\nperl: {:?}\nrust: {:?}",
            perl.log_oneline, rust.log_oneline
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
        run(
            &wc,
            "svn",
            &["commit", "--non-interactive", "-m", "add trunk file"],
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
    let rev_map = supported_rev_map(work_tree)?;
    let file_modes = supported_file_modes(work_tree, rev)?;
    let first_revision = rev_map
        .first()
        .ok_or_else(|| "golden clone did not write a rev_map record".to_string())?
        .revision;
    let log_oneline = supported_log_oneline(work_tree, tool)?;
    let find_rev = supported_find_rev(work_tree, tool, first_revision)?;
    let info_url = supported_info_url(work_tree, tool)?;
    let info_summary = supported_info_summary(work_tree, tool)?;
    let log_revision_oneline = supported_log_revision_oneline(work_tree, tool, first_revision)?;
    let find_rev_nearest = supported_find_rev_nearest(work_tree, tool, first_revision + 1)?;
    let find_rev_commit = supported_find_rev_commit(work_tree, tool, rev, first_revision)?;

    Ok(GoldenComparisonArtifacts {
        config,
        refs,
        git_svn_id_footers,
        rev_map,
        file_modes,
        log_oneline,
        find_rev,
        info_url,
        info_summary,
        log_revision_oneline,
        find_rev_nearest,
        find_rev_commit,
    })
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
            },
        )?,
    };
    Ok(normalize_log_oneline(&output))
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

fn supported_config(work_tree: &Path) -> Result<Vec<(String, String)>, String> {
    let keys = ["svn-remote.svn.url", "svn-remote.svn.fetch"];
    let mut config = Vec::new();
    for key in keys {
        let values = run_text(work_tree, "git", &["config", "--get-all", key])?;
        config.extend(values.lines().map(|value| {
            (
                key.to_string(),
                normalize_config_value(key, value).to_string(),
            )
        }));
    }
    config.sort();
    Ok(config)
}

fn normalize_config_value<'a>(key: &str, value: &'a str) -> &'a str {
    if key == "svn-remote.svn.fetch" {
        value.trim_start_matches('+')
    } else {
        value
    }
}

fn supported_rev_map(work_tree: &Path) -> Result<Vec<RevMapArtifactRecord>, String> {
    let svn_dir = work_tree.join(".git").join("svn");
    let mut paths = Vec::new();
    collect_rev_map_paths(&svn_dir, &mut paths)?;
    paths.sort();
    let path = paths
        .first()
        .ok_or_else(|| format!("missing .rev_map under {}", svn_dir.display()))?;
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() % 24 != 0 {
        return Err(format!("unsupported SHA-1 .rev_map size: {}", bytes.len()));
    }

    Ok(bytes
        .chunks_exact(24)
        .map(|record| RevMapArtifactRecord {
            revision: u32::from_be_bytes([record[0], record[1], record[2], record[3]]),
            has_commit: record[4..].iter().any(|byte| *byte != 0),
        })
        .filter(|record| record.has_commit)
        .collect())
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
        config_dir: None,
        no_auth_cache: false,
        preserve_empty_dirs: false,
        placeholder_filename: ".gitignore".to_string(),
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
        .map(|record| format!("{} {}", record.revision, record.has_commit))
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
}
