use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub trait GitBackend {
    fn init(&self) -> Result<(), String>;
    fn git_dir(&self) -> Result<String, String>;
    fn config_set(&self, key: &str, value: &str) -> Result<(), String>;
    fn config_add(&self, key: &str, value: &str) -> Result<(), String>;
    fn config_get(&self, key: &str) -> Result<Option<String>, String>;
    fn config_get_all(&self, key: &str) -> Result<Vec<String>, String>;
    fn fast_import(&self, input: &[u8]) -> Result<(), String>;
}

#[derive(Debug, Clone)]
pub struct GitCli {
    work_tree: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommitSummary {
    pub id: String,
    pub short_id: String,
    pub subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitNameStatus {
    pub status: String,
    pub path: String,
    pub old_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitTreeEntry {
    pub mode: String,
}

impl GitCli {
    pub fn new(work_tree: impl Into<PathBuf>) -> Self {
        Self {
            work_tree: work_tree.into(),
        }
    }

    pub fn work_tree(&self) -> &Path {
        &self.work_tree
    }

    pub fn init(&self) -> Result<(), String> {
        <Self as GitBackend>::init(self)
    }

    pub fn git_dir(&self) -> Result<String, String> {
        <Self as GitBackend>::git_dir(self)
    }

    pub fn config_set(&self, key: &str, value: &str) -> Result<(), String> {
        <Self as GitBackend>::config_set(self, key, value)
    }

    pub fn config_add(&self, key: &str, value: &str) -> Result<(), String> {
        <Self as GitBackend>::config_add(self, key, value)
    }

    pub fn config_get(&self, key: &str) -> Result<Option<String>, String> {
        <Self as GitBackend>::config_get(self, key)
    }

    pub fn config_get_all(&self, key: &str) -> Result<Vec<String>, String> {
        <Self as GitBackend>::config_get_all(self, key)
    }

    pub fn config_names_matching(&self, pattern: &str) -> Result<Vec<String>, String> {
        let output = Command::new("git")
            .current_dir(&self.work_tree)
            .args(["config", "--name-only", "--get-regexp", pattern])
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|line| line.to_string())
                .collect())
        } else if output.status.code() == Some(1) {
            Ok(Vec::new())
        } else {
            Err(stderr_or_status(output))
        }
    }

    pub fn fast_import(&self, input: &[u8]) -> Result<(), String> {
        <Self as GitBackend>::fast_import(self, input)
    }

    pub fn rev_parse(&self, rev: &str) -> Result<String, String> {
        self.run_args(["rev-parse", rev])
    }

    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool, String> {
        let output = Command::new("git")
            .current_dir(&self.work_tree)
            .args(["merge-base", "--is-ancestor", ancestor, descendant])
            .output()
            .map_err(|e| e.to_string())?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(stderr_or_status(output)),
        }
    }

    pub fn log_records(
        &self,
        rev: &str,
        limit: Option<u32>,
        passthrough_args: &[String],
    ) -> Result<String, String> {
        let mut args = vec![
            "log".to_string(),
            "--format=%H%x1f%an%x1f%aI%x1f%B%x1e".to_string(),
        ];
        if let Some(limit) = limit {
            args.push(format!("-n{limit}"));
        }
        args.push(rev.to_string());
        args.extend(passthrough_args.iter().cloned());
        self.run_args(args)
    }

    pub fn commit_summaries_between(
        &self,
        base: &str,
        head: &str,
    ) -> Result<Vec<GitCommitSummary>, String> {
        let rev_range = format!("{base}..{head}");
        let raw = self.run_args([
            "log",
            "--reverse",
            "--format=%H%x1f%h%x1f%s%x1e",
            &rev_range,
        ])?;
        let mut commits = Vec::new();
        for record in raw.split('\x1e') {
            let record = record.trim_matches('\n');
            if record.is_empty() {
                continue;
            }
            let fields = record.splitn(3, '\x1f').collect::<Vec<_>>();
            if fields.len() != 3 {
                return Err(format!("unexpected git log record: {record}"));
            }
            commits.push(GitCommitSummary {
                id: fields[0].to_string(),
                short_id: fields[1].to_string(),
                subject: fields[2].to_string(),
            });
        }
        Ok(commits)
    }

    pub fn commit_author(&self, commit: &str) -> Result<String, String> {
        self.run_args(["show", "-s", "--format=%an", commit])
            .map(|author| author.trim().to_string())
    }

    pub fn update_ref(&self, refname: &str, value: &str) -> Result<(), String> {
        self.run(["update-ref", refname, value]).map(|_| ())
    }

    pub fn diff_name_status(&self, base: &str, commit: &str) -> Result<Vec<GitNameStatus>, String> {
        let raw = self.run_bytes([
            "diff-tree",
            "--no-commit-id",
            "--name-status",
            "-M",
            "-C",
            "--find-copies-harder",
            "-r",
            "-z",
            base,
            commit,
        ])?;
        parse_name_status(&raw)
    }

    pub fn commit_name_status(&self, commit: &str) -> Result<Vec<GitNameStatus>, String> {
        let raw = self.run_bytes([
            "diff-tree",
            "--root",
            "--no-commit-id",
            "--name-status",
            "-M",
            "-C",
            "--find-copies-harder",
            "-r",
            "-z",
            commit,
        ])?;
        parse_name_status(&raw)
    }

    pub fn show_file(&self, commit: &str, path: &str) -> Result<Vec<u8>, String> {
        let spec = format!("{commit}:{path}");
        self.run_bytes(["show", &spec])
    }

    pub fn ls_tree_file(&self, commit: &str, path: &str) -> Result<GitTreeEntry, String> {
        let raw = self.run_bytes(["ls-tree", "-z", commit, "--", path])?;
        parse_ls_tree_file(&raw, path)
    }

    pub fn rebase(
        &self,
        upstream: &str,
        merge: bool,
        strategy: Option<&str>,
    ) -> Result<String, String> {
        let mut args = vec!["rebase".to_string()];
        if merge {
            args.push("--merge".to_string());
        }
        if let Some(strategy) = strategy {
            args.push("--strategy".to_string());
            args.push(strategy.to_string());
        }
        args.push(upstream.to_string());
        self.run_args(args)
    }

    pub fn run_for_test<const N: usize>(&self, args: [&str; N]) -> Result<String, String> {
        self.run(args)
    }

    fn run<const N: usize>(&self, args: [&str; N]) -> Result<String, String> {
        self.run_args(args)
    }

    fn run_args<I, S>(&self, args: I) -> Result<String, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let output = Command::new("git")
            .current_dir(&self.work_tree)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        command_output(output)
    }

    fn run_bytes<I, S>(&self, args: I) -> Result<Vec<u8>, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let output = Command::new("git")
            .current_dir(&self.work_tree)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(stderr_or_status(output))
        }
    }
}

impl GitBackend for GitCli {
    fn init(&self) -> Result<(), String> {
        self.run(["init"]).map(|_| ())
    }

    fn git_dir(&self) -> Result<String, String> {
        self.run(["rev-parse", "--git-dir"])
            .map(|s| s.trim().to_string())
    }

    fn config_set(&self, key: &str, value: &str) -> Result<(), String> {
        self.run(["config", key, value]).map(|_| ())
    }

    fn config_add(&self, key: &str, value: &str) -> Result<(), String> {
        self.run(["config", "--add", key, value]).map(|_| ())
    }

    fn config_get(&self, key: &str) -> Result<Option<String>, String> {
        let output = Command::new("git")
            .current_dir(&self.work_tree)
            .args(["config", "--get", key])
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(Some(
                String::from_utf8_lossy(&output.stdout)
                    .trim_end()
                    .to_string(),
            ))
        } else if output.status.code() == Some(1) {
            Ok(None)
        } else {
            Err(stderr_or_status(output))
        }
    }

    fn config_get_all(&self, key: &str) -> Result<Vec<String>, String> {
        let output = Command::new("git")
            .current_dir(&self.work_tree)
            .args(["config", "--get-all", key])
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|line| line.to_string())
                .collect())
        } else if output.status.code() == Some(1) {
            Ok(Vec::new())
        } else {
            Err(stderr_or_status(output))
        }
    }

    fn fast_import(&self, input: &[u8]) -> Result<(), String> {
        let mut child = Command::new("git")
            .current_dir(&self.work_tree)
            .args(["fast-import", "--quiet"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;

        child
            .stdin
            .as_mut()
            .ok_or_else(|| "failed to open git fast-import stdin".to_string())?
            .write_all(input)
            .map_err(|e| e.to_string())?;

        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        command_output(output).map(|_| ())
    }
}

fn command_output(output: std::process::Output) -> Result<String, String> {
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(stderr_or_status(output))
    }
}

fn stderr_or_status(output: std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        format!("git exited with status {}", output.status)
    } else {
        stderr
    }
}

fn parse_name_status(raw: &[u8]) -> Result<Vec<GitNameStatus>, String> {
    let mut parts = raw.split(|byte| *byte == 0).filter(|part| !part.is_empty());
    let mut changes = Vec::new();

    while let Some(status) = parts.next() {
        let status = String::from_utf8(status.to_vec()).map_err(|e| e.to_string())?;
        let path = parts
            .next()
            .ok_or_else(|| format!("missing path for git diff status {status}"))?;
        let path = String::from_utf8(path.to_vec()).map_err(|e| e.to_string())?;
        if status.starts_with('R') || status.starts_with('C') {
            let new_path = parts
                .next()
                .ok_or_else(|| format!("missing destination path for git diff status {status}"))?;
            changes.push(GitNameStatus {
                status,
                path: String::from_utf8(new_path.to_vec()).map_err(|e| e.to_string())?,
                old_path: Some(path),
            });
        } else {
            changes.push(GitNameStatus {
                status,
                path,
                old_path: None,
            });
        }
    }

    Ok(changes)
}

fn parse_ls_tree_file(raw: &[u8], path: &str) -> Result<GitTreeEntry, String> {
    let entry = raw
        .split(|byte| *byte == 0)
        .find(|entry| !entry.is_empty())
        .ok_or_else(|| format!("missing git tree entry for {path}"))?;
    let text = String::from_utf8(entry.to_vec()).map_err(|e| e.to_string())?;
    let (mode, _) = text
        .split_once(' ')
        .ok_or_else(|| format!("unexpected git ls-tree entry: {text}"))?;
    Ok(GitTreeEntry {
        mode: mode.to_string(),
    })
}
