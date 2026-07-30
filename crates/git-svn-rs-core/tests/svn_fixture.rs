mod support;

use std::path::Path;
use std::process::Command;

use git_svn_rs_core::svn::ChangeAction;
use git_svn_rs_core::svn::SvnBackend;
use git_svn_rs_core::svn::cli::SvnCliBackend;
use git_svn_rs_core::svn::ra::{RaSession, SvnNodeKind};
use support::svn_fixture::{
    StandardSvnFixture, SvnToolPolicy, missing_tools_policy, require_svn_tools,
};

#[test]
fn missing_svn_tools_are_skipped_unless_strict_compat_is_set() {
    assert_eq!(
        missing_tools_policy(false),
        SvnToolPolicy::Skip("skipping: svnadmin and svn are required".to_string())
    );
    assert_eq!(
        missing_tools_policy(true),
        SvnToolPolicy::Fail("svnadmin and svn are required".to_string())
    );
}

#[test]
fn standard_fixture_creates_trunk_branch_and_tag_revisions() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let fixture = StandardSvnFixture::create().unwrap();

    assert!(fixture.url().starts_with("file:///"));
    assert!(fixture.latest_revision() >= 4);
}

#[test]
fn svn_cli_get_dir_returns_immediate_files_and_directories() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let fixture = StandardSvnFixture::create().unwrap();
    let backend = SvnCliBackend::new(fixture.url()).unwrap();
    let listing = backend.get_dir("trunk", 2).unwrap();

    assert_eq!(listing.entries["src"].kind, SvnNodeKind::Directory);
    assert_eq!(listing.entries["empty-dir"].kind, SvnNodeKind::Directory);
    assert_eq!(listing.entries["run.sh"].kind, SvnNodeKind::File);
    assert!(!listing.entries.contains_key("src/lib.rs"));
}

#[test]
fn svn_cli_log_handles_deleted_files_after_copies_without_catting_them() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let fixture = DeletedFileAfterCopyFixture::create().unwrap();
    let backend = SvnCliBackend::new(fixture.url()).unwrap();

    let revisions = backend.log(1, 5).unwrap();

    assert!(revisions.iter().any(|revision| {
        revision.revision == 5
            && revision.changed_paths.iter().any(|path| {
                path.path == "/trunk/deleted.txt" && path.action == ChangeAction::Delete
            })
    }));
}

struct DeletedFileAfterCopyFixture {
    _tmp: tempfile::TempDir,
    repo: std::path::PathBuf,
}

impl DeletedFileAfterCopyFixture {
    fn create() -> Result<Self, String> {
        let tmp = tempfile::Builder::new()
            .prefix("svn-delete-fixture-")
            .tempdir_in(std::env::current_dir().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let repo = tmp.path().join("repo");
        let wc = tmp.path().join("wc");

        run(tmp.path(), "svnadmin", &["create", path_arg(&repo)?])?;
        let url = file_url(&repo)?;
        run(
            tmp.path(),
            "svn",
            &["checkout", "--non-interactive", &url, path_arg(&wc)?],
        )?;
        run(
            &wc,
            "svn",
            &["mkdir", "--non-interactive", "trunk", "branches", "tags"],
        )?;
        run(&wc, "svn", &["commit", "--non-interactive", "-m", "layout"])?;

        std::fs::write(wc.join("trunk/deleted.txt"), "temporary\n").map_err(|e| e.to_string())?;
        run(
            &wc,
            "svn",
            &["add", "--non-interactive", "trunk/deleted.txt"],
        )?;
        run(
            &wc,
            "svn",
            &["commit", "--non-interactive", "-m", "add temporary file"],
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

        Ok(Self { _tmp: tmp, repo })
    }

    fn url(&self) -> String {
        file_url(&self.repo).expect("fixture repository path should convert to file URL")
    }
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
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "{program} failed with status {}: {}{}",
            output.status,
            stderr.trim(),
            if stdout.trim().is_empty() {
                String::new()
            } else {
                format!(" stdout: {}", stdout.trim())
            }
        ))
    }
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
