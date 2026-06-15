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

    pub fn fast_import(&self, input: &[u8]) -> Result<(), String> {
        <Self as GitBackend>::fast_import(self, input)
    }

    pub fn rev_parse(&self, rev: &str) -> Result<String, String> {
        self.run_args(["rev-parse", rev])
    }

    pub fn log_records(&self, rev: &str, limit: Option<u32>) -> Result<String, String> {
        let mut args = vec![
            "log".to_string(),
            "--reverse".to_string(),
            "--format=%H%x1f%an%x1f%aI%x1f%B%x1e".to_string(),
        ];
        if let Some(limit) = limit {
            args.push(format!("-n{limit}"));
        }
        args.push(rev.to_string());
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

    pub fn update_ref(&self, refname: &str, value: &str) -> Result<(), String> {
        self.run(["update-ref", refname, value]).map(|_| ())
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
