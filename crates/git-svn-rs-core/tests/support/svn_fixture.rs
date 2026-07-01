use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

const MISSING_TOOLS: &str = "svnadmin and svn are required";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SvnToolPolicy {
    Skip(String),
    Fail(String),
}

pub fn has_svn_tools() -> bool {
    command_succeeds("svnadmin", &["--version"]) && command_succeeds("svn", &["--version"])
}

pub fn require_svn_tools() -> Result<(), SvnToolPolicy> {
    if has_svn_tools() {
        Ok(())
    } else {
        Err(missing_tools_policy(strict_compat()))
    }
}

pub fn missing_tools_policy(strict_compat: bool) -> SvnToolPolicy {
    if strict_compat {
        SvnToolPolicy::Fail(MISSING_TOOLS.to_string())
    } else {
        SvnToolPolicy::Skip(format!("skipping: {MISSING_TOOLS}"))
    }
}

pub struct StandardSvnFixture {
    _tmp: TempDir,
    repo: PathBuf,
}

impl StandardSvnFixture {
    pub fn create() -> Result<Self, String> {
        let tmp = tempfile::Builder::new()
            .prefix("svn-fixture-")
            .tempdir_in(std::env::current_dir().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let repo = tmp.path().join("repo");
        let wc = tmp.path().join("wc");

        run(tmp.path(), "svnadmin", &["create", path_arg(&repo)?])?;

        let url = file_url(&repo)?;
        run(
            tmp.path(),
            "svn",
            &[
                "checkout",
                "--non-interactive",
                url.as_str(),
                path_arg(&wc)?,
            ],
        )?;
        run(
            &wc,
            "svn",
            &["mkdir", "--non-interactive", "trunk", "branches", "tags"],
        )?;
        run(&wc, "svn", &["commit", "--non-interactive", "-m", "layout"])?;

        std::fs::create_dir_all(wc.join("trunk/src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(wc.join("trunk/empty-dir")).map_err(|e| e.to_string())?;
        std::fs::write(
            wc.join("trunk/src/lib.rs"),
            "pub fn answer() -> u8 { 42 }\n",
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(wc.join("trunk/run.sh"), "#!/bin/sh\necho hi\n")
            .map_err(|e| e.to_string())?;
        std::fs::write(wc.join("trunk/link-to-lib"), "link src/lib.rs")
            .map_err(|e| e.to_string())?;
        run(
            &wc,
            "svn",
            &[
                "add",
                "--non-interactive",
                "trunk/src",
                "trunk/run.sh",
                "trunk/link-to-lib",
                "trunk/empty-dir",
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

        Ok(Self { _tmp: tmp, repo })
    }

    pub fn url(&self) -> String {
        file_url(&self.repo).expect("fixture repository path should convert to file URL")
    }

    #[allow(dead_code)]
    pub fn root(&self) -> &Path {
        self.repo
            .parent()
            .expect("fixture repository should have a parent")
    }

    pub fn latest_revision(&self) -> u32 {
        let output = Command::new("svn")
            .args(["info", "--show-item", "revision", self.url().as_str()])
            .output()
            .expect("svn info should run");
        assert!(
            output.status.success(),
            "svn info failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .expect("svn info revision should be numeric")
    }

    #[allow(dead_code)]
    pub fn uuid(&self) -> String {
        let output = Command::new("svn")
            .args(["info", "--show-item", "repos-uuid", self.url().as_str()])
            .output()
            .expect("svn info should run");
        assert!(
            output.status.success(),
            "svn info failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }
}

fn command_succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn strict_compat() -> bool {
    std::env::var("GIT_SVN_RS_STRICT_COMPAT").as_deref() == Ok("1")
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
