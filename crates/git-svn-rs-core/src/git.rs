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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitRawDiffStatus {
    Added,
    Modified,
    TypeChanged,
    Deleted,
    Renamed,
    Copied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRawDiffEntry {
    pub old_mode: String,
    pub new_mode: String,
    pub old_oid: String,
    pub new_oid: String,
    pub status: GitRawDiffStatus,
    pub similarity: Option<u8>,
    pub source_path: Option<String>,
    pub target_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitTreeEntry {
    pub mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitTreeFile {
    pub path: String,
    pub mode: String,
    pub content: Vec<u8>,
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

    pub fn refs_under(&self, prefix: &str) -> Result<Vec<String>, String> {
        Ok(self
            .run_args(["for-each-ref", prefix, "--format=%(refname)"])?
            .lines()
            .map(str::to_string)
            .collect())
    }

    pub fn fast_import(&self, input: &[u8]) -> Result<(), String> {
        <Self as GitBackend>::fast_import(self, input)
    }

    pub fn rev_parse(&self, rev: &str) -> Result<String, String> {
        self.run_args(["rev-parse", rev])
    }

    pub fn object_format(&self) -> Result<crate::rev_map::ObjectFormat, String> {
        match self.run_args(["rev-parse", "--show-object-format"])?.trim() {
            "sha1" => Ok(crate::rev_map::ObjectFormat::Sha1),
            "sha256" => Ok(crate::rev_map::ObjectFormat::Sha256),
            format => Err(format!("unsupported Git object format: {format}")),
        }
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

    pub fn first_parent_history(&self, rev: &str) -> Result<Vec<String>, String> {
        Ok(self
            .run_args(["rev-list", "--first-parent", rev])?
            .lines()
            .map(str::to_string)
            .collect())
    }

    pub fn range_has_merges(&self, base: &str, head: &str) -> Result<bool, String> {
        let range = format!("{base}..{head}");
        Ok(!self
            .run_args(["rev-list", "--merges", "--max-count=1", &range])?
            .trim()
            .is_empty())
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

    pub fn commit_message(&self, commit: &str) -> Result<String, String> {
        self.run_args(["show", "-s", "--format=%B", commit])
    }

    pub fn update_ref(&self, refname: &str, value: &str) -> Result<(), String> {
        self.run(["update-ref", refname, value]).map(|_| ())
    }

    pub fn materialize_initial_branch(
        &self,
        tracking_ref: &str,
        no_checkout: bool,
    ) -> Result<(), String> {
        let exists = Command::new("git")
            .current_dir(&self.work_tree)
            .args(["show-ref", "--verify", "--quiet", tracking_ref])
            .status()
            .map_err(|e| e.to_string())?;
        match exists.code() {
            Some(0) => {}
            Some(1) => return Ok(()),
            _ => return Err(format!("git show-ref exited with status {exists}")),
        }

        if no_checkout {
            self.run(["update-ref", "HEAD", tracking_ref]).map(|_| ())
        } else {
            self.run(["reset", "--hard", tracking_ref]).map(|_| ())
        }
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

    pub fn diff_raw(&self, base: &str, commit: &str) -> Result<Vec<GitRawDiffEntry>, String> {
        let raw = self.run_bytes([
            "diff-tree",
            "--no-commit-id",
            "--raw",
            "-z",
            "-r",
            "-M",
            "-C",
            "--find-copies-harder",
            base,
            commit,
        ])?;
        parse_raw_diff(&raw)
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

    pub fn tree_files(&self, commit: &str) -> Result<Vec<GitTreeFile>, String> {
        let raw = self.run_bytes(["ls-tree", "-r", "-z", commit])?;
        let entries = parse_ls_tree_files(&raw)?;
        entries
            .into_iter()
            .map(|(path, mode)| {
                let content = self.show_file(commit, &path)?;
                Ok(GitTreeFile {
                    path,
                    mode,
                    content,
                })
            })
            .collect()
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

fn parse_raw_diff(raw: &[u8]) -> Result<Vec<GitRawDiffEntry>, String> {
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    if raw.last() != Some(&0) {
        return Err("unterminated git raw diff output".to_string());
    }

    let fields = raw[..raw.len() - 1]
        .split(|byte| *byte == 0)
        .collect::<Vec<_>>();
    let mut index = 0;
    let mut changes = Vec::new();
    while index < fields.len() {
        let header = std::str::from_utf8(fields[index]).map_err(|e| e.to_string())?;
        index += 1;
        let metadata = header
            .strip_prefix(':')
            .ok_or_else(|| format!("unexpected git raw diff header: {header}"))?
            .split_whitespace()
            .collect::<Vec<_>>();
        if metadata.len() != 5 {
            return Err(format!("unexpected git raw diff header: {header}"));
        }

        let (status, similarity, path_count) = parse_raw_diff_status(metadata[4])?;
        if fields.len() - index < path_count {
            return Err(format!(
                "missing path for git raw diff status {}",
                metadata[4]
            ));
        }
        let first_path = parse_raw_diff_path(fields[index], metadata[4])?;
        index += 1;

        let (source_path, target_path) = match status {
            GitRawDiffStatus::Added => (None, Some(first_path)),
            GitRawDiffStatus::Deleted => (Some(first_path), None),
            GitRawDiffStatus::Modified | GitRawDiffStatus::TypeChanged => {
                (Some(first_path.clone()), Some(first_path))
            }
            GitRawDiffStatus::Renamed | GitRawDiffStatus::Copied => {
                let target_path = parse_raw_diff_path(fields[index], metadata[4])?;
                index += 1;
                (Some(first_path), Some(target_path))
            }
        };

        changes.push(GitRawDiffEntry {
            old_mode: metadata[0].to_string(),
            new_mode: metadata[1].to_string(),
            old_oid: metadata[2].to_string(),
            new_oid: metadata[3].to_string(),
            status,
            similarity,
            source_path,
            target_path,
        });
    }
    Ok(changes)
}

fn parse_raw_diff_status(status: &str) -> Result<(GitRawDiffStatus, Option<u8>, usize), String> {
    match status {
        "A" => Ok((GitRawDiffStatus::Added, None, 1)),
        "M" => Ok((GitRawDiffStatus::Modified, None, 1)),
        "T" => Ok((GitRawDiffStatus::TypeChanged, None, 1)),
        "D" => Ok((GitRawDiffStatus::Deleted, None, 1)),
        _ => {
            let (kind, score) = if let Some(score) = status.strip_prefix('R') {
                (GitRawDiffStatus::Renamed, score)
            } else if let Some(score) = status.strip_prefix('C') {
                (GitRawDiffStatus::Copied, score)
            } else {
                return Err(format!("unsupported git raw diff status: {status}"));
            };
            if score.is_empty() || !score.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(format!("invalid git raw diff similarity: {status}"));
            }
            let score = score
                .parse::<u8>()
                .map_err(|_| format!("invalid git raw diff similarity: {status}"))?;
            if score > 100 {
                return Err(format!("invalid git raw diff similarity: {status}"));
            }
            Ok((kind, Some(score), 2))
        }
    }
}

fn parse_raw_diff_path(path: &[u8], status: &str) -> Result<String, String> {
    if path.is_empty() {
        return Err(format!("empty path for git raw diff status {status}"));
    }
    String::from_utf8(path.to_vec()).map_err(|e| e.to_string())
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

fn parse_ls_tree_files(raw: &[u8]) -> Result<Vec<(String, String)>, String> {
    raw.split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let text = String::from_utf8(entry.to_vec()).map_err(|e| e.to_string())?;
            let (metadata, path) = text
                .split_once('\t')
                .ok_or_else(|| format!("unexpected git ls-tree entry: {text}"))?;
            let mode = metadata
                .split_whitespace()
                .next()
                .ok_or_else(|| format!("unexpected git ls-tree metadata: {metadata}"))?;
            Ok((path.to_string(), mode.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{GitCli, GitRawDiffEntry, GitRawDiffStatus, parse_raw_diff};

    #[test]
    fn parses_raw_diff_add_modify_type_change_and_delete_records() {
        let raw = concat!(
            ":000000 100644 0000000 1111111 A\0added.txt\0",
            ":100644 100755 2222222 3333333 M\0script.sh\0",
            ":100644 120000 3333333 4444444 T\0link.txt\0",
            ":100644 000000 4444444 0000000 D\0removed.txt\0"
        );

        assert_eq!(
            parse_raw_diff(raw.as_bytes()).unwrap(),
            vec![
                GitRawDiffEntry {
                    old_mode: "000000".to_string(),
                    new_mode: "100644".to_string(),
                    old_oid: "0000000".to_string(),
                    new_oid: "1111111".to_string(),
                    status: GitRawDiffStatus::Added,
                    similarity: None,
                    source_path: None,
                    target_path: Some("added.txt".to_string()),
                },
                GitRawDiffEntry {
                    old_mode: "100644".to_string(),
                    new_mode: "100755".to_string(),
                    old_oid: "2222222".to_string(),
                    new_oid: "3333333".to_string(),
                    status: GitRawDiffStatus::Modified,
                    similarity: None,
                    source_path: Some("script.sh".to_string()),
                    target_path: Some("script.sh".to_string()),
                },
                GitRawDiffEntry {
                    old_mode: "100644".to_string(),
                    new_mode: "120000".to_string(),
                    old_oid: "3333333".to_string(),
                    new_oid: "4444444".to_string(),
                    status: GitRawDiffStatus::TypeChanged,
                    similarity: None,
                    source_path: Some("link.txt".to_string()),
                    target_path: Some("link.txt".to_string()),
                },
                GitRawDiffEntry {
                    old_mode: "100644".to_string(),
                    new_mode: "000000".to_string(),
                    old_oid: "4444444".to_string(),
                    new_oid: "0000000".to_string(),
                    status: GitRawDiffStatus::Deleted,
                    similarity: None,
                    source_path: Some("removed.txt".to_string()),
                    target_path: None,
                },
            ]
        );
    }

    #[test]
    fn parses_raw_diff_rename_and_copy_records() {
        let raw = concat!(
            ":100644 100644 1111111 1111111 R087\0old name.txt\0new name.txt\0",
            ":100755 100755 2222222 2222222 C100\0source.sh\0copy.sh\0"
        );

        let changes = parse_raw_diff(raw.as_bytes()).unwrap();
        assert_eq!(changes[0].status, GitRawDiffStatus::Renamed);
        assert_eq!(changes[0].similarity, Some(87));
        assert_eq!(changes[0].source_path.as_deref(), Some("old name.txt"));
        assert_eq!(changes[0].target_path.as_deref(), Some("new name.txt"));
        assert_eq!(changes[1].status, GitRawDiffStatus::Copied);
        assert_eq!(changes[1].similarity, Some(100));
        assert_eq!(changes[1].source_path.as_deref(), Some("source.sh"));
        assert_eq!(changes[1].target_path.as_deref(), Some("copy.sh"));
    }

    #[test]
    fn rejects_malformed_raw_diff_records() {
        for (raw, expected) in [
            (
                b":100644 100644 1111111 2222222 M\0path".as_slice(),
                "unterminated",
            ),
            (b"100644 100644 1111111 2222222 M\0path\0", "header"),
            (b":100644 100644 1111111 M\0path\0", "header"),
            (
                b":100644 100644 1111111 2222222 R\0old\0new\0",
                "similarity",
            ),
            (
                b":100644 100644 1111111 2222222 R101\0old\0new\0",
                "similarity",
            ),
            (b":100644 100644 1111111 2222222 X\0path\0", "unsupported"),
            (
                b":100644 100644 1111111 2222222 C050\0source\0",
                "missing path",
            ),
            (b":100644 100644 1111111 2222222 A\0\0", "empty path"),
            (
                b":100644 120000 1111111 2222222 T\0invalid-\xff\0",
                "invalid utf-8",
            ),
        ] {
            let error = parse_raw_diff(raw).unwrap_err();
            assert!(error.contains(expected), "unexpected error: {error}");
        }
    }

    #[test]
    fn git_cli_reads_typed_raw_diff() {
        let dir = tempfile::tempdir().unwrap();
        let git = GitCli::new(dir.path());
        git.init().unwrap();
        git.run_for_test(["config", "user.name", "Test User"])
            .unwrap();
        git.run_for_test(["config", "user.email", "test@example.com"])
            .unwrap();
        std::fs::write(dir.path().join("original.txt"), "unchanged\n").unwrap();
        std::fs::write(dir.path().join("modified.txt"), "before\n").unwrap();
        std::fs::write(dir.path().join("type-change.txt"), "regular\n").unwrap();
        git.run_for_test(["add", "."]).unwrap();
        git.run_for_test(["commit", "-m", "base"]).unwrap();
        let base = git.rev_parse("HEAD").unwrap().trim().to_string();

        std::fs::write(dir.path().join("modified.txt"), "after\n").unwrap();
        std::fs::copy(
            dir.path().join("original.txt"),
            dir.path().join("copied.txt"),
        )
        .unwrap();
        git.run_for_test(["add", "."]).unwrap();
        std::fs::write(dir.path().join("link-target"), "destination\n").unwrap();
        let link_oid = git
            .run_for_test(["hash-object", "-w", "link-target"])
            .unwrap();
        let cache_info = format!("120000,{},type-change.txt", link_oid.trim());
        git.run_for_test(["update-index", "--cacheinfo", &cache_info])
            .unwrap();
        git.run_for_test(["commit", "-m", "changes"]).unwrap();

        let changes = git.diff_raw(&base, "HEAD").unwrap();
        assert!(changes.iter().any(|change| {
            change.status == GitRawDiffStatus::Modified
                && change.target_path.as_deref() == Some("modified.txt")
        }));
        assert!(changes.iter().any(|change| {
            change.status == GitRawDiffStatus::Copied
                && change.source_path.as_deref() == Some("original.txt")
                && change.target_path.as_deref() == Some("copied.txt")
                && change.similarity == Some(100)
        }));
        assert!(changes.iter().any(|change| {
            change.status == GitRawDiffStatus::TypeChanged
                && change.old_mode == "100644"
                && change.new_mode == "120000"
                && change.source_path.as_deref() == Some("type-change.txt")
                && change.target_path.as_deref() == Some("type-change.txt")
                && change.similarity.is_none()
        }));
    }
}
