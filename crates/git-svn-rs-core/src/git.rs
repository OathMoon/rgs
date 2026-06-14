use std::path::{Path, PathBuf};
use std::process::Command;

pub trait GitBackend {
    fn init(&self) -> Result<(), String>;
    fn git_dir(&self) -> Result<String, String>;
    fn config_set(&self, key: &str, value: &str) -> Result<(), String>;
    fn config_add(&self, key: &str, value: &str) -> Result<(), String>;
    fn config_get(&self, key: &str) -> Result<Option<String>, String>;
    fn config_get_all(&self, key: &str) -> Result<Vec<String>, String>;
}

#[derive(Debug, Clone)]
pub struct GitCli {
    work_tree: PathBuf,
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

    pub fn run_for_test<const N: usize>(&self, args: [&str; N]) -> Result<String, String> {
        self.run(args)
    }

    fn run<const N: usize>(&self, args: [&str; N]) -> Result<String, String> {
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
