use crate::cli::DcommitArgs;
use crate::commands::resolver::resolve_tracked_svn;
use crate::commands::{fetch, rebase};
use crate::dcommit::{GitDiffChange, GitDiffPlanner, PropertyMapper, SvnCommitEditor};
use crate::git::{GitCli, GitCommitSummary};
use crate::rev_map::RevMap;
use crate::svn::CommitRecord;
use crate::svn::mock::MockSvnBackend;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn run(args: DcommitArgs) -> Result<String, String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: DcommitArgs,
) -> Result<String, String> {
    let tracked = resolve_tracked_svn(work_tree)?;
    let revision = tracked
        .max_record()?
        .map(|record| record.revision)
        .unwrap_or(0);
    let target_url = args.commit_url.as_deref().unwrap_or(&tracked.config.url);
    let commits = if tracked.git.rev_parse("HEAD").is_ok() {
        tracked
            .git
            .commit_summaries_between(&tracked.refname, "HEAD")?
    } else {
        Vec::new()
    };

    if !args.dry_run {
        if target_url.starts_with("mock://") && tracked.config.url.starts_with("mock://") {
            if args.mergeinfo.is_some() {
                return Err(
                    "--mergeinfo write-back is only implemented for file:// URLs in v1".to_string(),
                );
            }
            return dcommit_mock(
                &tracked.git,
                &tracked.refname,
                tracked.open_rev_map()?,
                &tracked.uuid,
                revision,
                commits,
                args.no_rebase,
            );
        }
        if target_url.starts_with("file://") && tracked.config.url.starts_with("file://") {
            let commit_svn_path = if args.commit_url.is_some() {
                ""
            } else {
                &tracked.svn_path
            };
            return dcommit_file_svn(
                &tracked.git,
                target_url,
                commit_svn_path,
                &tracked.refname,
                commits,
                args.no_rebase,
                args.mergeinfo.as_deref(),
            );
        }
        {
            return Err(
                "dcommit write-back is only implemented for mock:// and file:// URLs; production remote SVN write-back is not implemented"
                    .to_string(),
            );
        }
    }

    let mut out = format!(
        "Dcommit dry-run against {target_url} ({}, r{revision})\n",
        tracked.refname
    );
    if let Some(mergeinfo) = &args.mergeinfo {
        out.push_str(&format!(
            "explicit mergeinfo accepted for dry-run: {mergeinfo}\n"
        ));
        out.push_str("automatic mergeinfo generation is not implemented in v1\n");
    }
    if commits.is_empty() {
        out.push_str("No local commits to dcommit.\n");
        return Ok(out);
    }

    out.push_str(&format!(
        "Would commit {} local Git commit(s):\n",
        commits.len()
    ));
    for commit in commits {
        out.push_str(&format!("{} {}\n", commit.short_id, commit.subject));
    }
    Ok(out)
}

fn dcommit_file_svn(
    git: &GitCli,
    svn_root_url: &str,
    svn_path: &str,
    refname: &str,
    commits: Vec<GitCommitSummary>,
    no_rebase: bool,
    mergeinfo: Option<&str>,
) -> Result<String, String> {
    if commits.is_empty() {
        return Ok("No local commits to dcommit.\n".to_string());
    }

    let temp = TempCheckout::new()?;
    let checkout_url = svn_checkout_url(svn_root_url, svn_path);
    run_svn(
        None,
        &[
            "checkout".to_string(),
            "--quiet".to_string(),
            checkout_url,
            temp.wc.display().to_string(),
        ],
    )?;

    let mut out = format!("Committed {} local Git commit(s)\n", commits.len());
    let mut diff_base = refname.to_string();
    for commit in &commits {
        let changes = git.diff_name_status(&diff_base, &commit.id)?;
        for change in changes {
            apply_file_svn_change(
                git,
                &temp.wc,
                &diff_base,
                &commit.id,
                &change.status,
                &change.path,
                change.old_path.as_deref(),
            )?;
        }
        if let Some(mergeinfo) = mergeinfo {
            apply_mergeinfo(&temp.wc, mergeinfo)?;
        }
        let revision = svn_commit(&temp.wc, &commit.subject)?;
        fetch::run_in_work_tree(git.work_tree().to_path_buf(), default_fetch_args())?;
        diff_base = commit.id.clone();
        out.push_str(&format!(
            "Committed {} {} as r{revision}\n",
            commit.short_id, commit.subject
        ));
    }

    if no_rebase {
        out.push_str("Skipped rebase (--no-rebase).\n");
    } else {
        out.push_str(&rebase::run_in_work_tree(
            git.work_tree().to_path_buf(),
            crate::cli::RebaseArgs {
                dry_run: false,
                merge: false,
                strategy: None,
                shared: default_shared_args(),
            },
        )?);
    }

    Ok(out)
}

fn apply_file_svn_change(
    git: &GitCli,
    wc: &Path,
    base: &str,
    commit: &str,
    status: &str,
    path: &str,
    old_path: Option<&str>,
) -> Result<(), String> {
    let target = wc.join(path);
    match status {
        "A" => {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&target, svn_file_content(git, commit, path)?)
                .map_err(|e| e.to_string())?;
            run_svn(
                Some(wc),
                &[
                    "add".to_string(),
                    "--parents".to_string(),
                    path.replace('\\', "/"),
                ],
            )?;
            apply_file_props(git, wc, commit, path, "000000")?;
        }
        "M" | "T" => {
            std::fs::write(&target, svn_file_content(git, commit, path)?)
                .map_err(|e| e.to_string())?;
            let old_mode = git.ls_tree_file(base, path)?.mode;
            apply_file_props(git, wc, commit, path, &old_mode)?;
        }
        "D" => {
            run_svn(Some(wc), &["delete".to_string(), path.replace('\\', "/")])?;
        }
        status if status.starts_with('R') => {
            let old_path = old_path.ok_or_else(|| format!("missing source path for {status}"))?;
            ensure_svn_parent_dirs(wc, path)?;
            run_svn(
                Some(wc),
                &[
                    "move".to_string(),
                    old_path.replace('\\', "/"),
                    path.replace('\\', "/"),
                ],
            )?;
            std::fs::write(&target, svn_file_content(git, commit, path)?)
                .map_err(|e| e.to_string())?;
            let old_mode = git.ls_tree_file(base, old_path)?.mode;
            apply_file_props(git, wc, commit, path, &old_mode)?;
        }
        status if status.starts_with('C') => {
            let old_path = old_path.ok_or_else(|| format!("missing source path for {status}"))?;
            ensure_svn_parent_dirs(wc, path)?;
            run_svn(
                Some(wc),
                &[
                    "copy".to_string(),
                    old_path.replace('\\', "/"),
                    path.replace('\\', "/"),
                ],
            )?;
            std::fs::write(&target, svn_file_content(git, commit, path)?)
                .map_err(|e| e.to_string())?;
            let old_mode = git.ls_tree_file(base, old_path)?.mode;
            apply_file_props(git, wc, commit, path, &old_mode)?;
        }
        other => {
            return Err(format!(
                "dcommit does not support git diff status {other} yet"
            ));
        }
    }
    Ok(())
}

fn ensure_svn_parent_dirs(wc: &Path, path: &str) -> Result<(), String> {
    let Some(parent) = Path::new(path).parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() {
        return Ok(());
    }

    let parent_path = wc.join(parent);
    if parent_path.exists() {
        return Ok(());
    }

    std::fs::create_dir_all(&parent_path).map_err(|e| e.to_string())?;
    run_svn(
        Some(wc),
        &[
            "add".to_string(),
            "--parents".to_string(),
            parent.to_string_lossy().replace('\\', "/"),
        ],
    )
}

fn svn_file_content(git: &GitCli, commit: &str, path: &str) -> Result<Vec<u8>, String> {
    let mode = git.ls_tree_file(commit, path)?.mode;
    let content = git.show_file(commit, path)?;
    if mode == "120000" {
        let mut special = b"link ".to_vec();
        special.extend(content);
        Ok(special)
    } else {
        Ok(content)
    }
}

fn apply_mergeinfo(wc: &Path, mergeinfo: &str) -> Result<(), String> {
    run_svn(
        Some(wc),
        &[
            "propset".to_string(),
            "--non-interactive".to_string(),
            "svn:mergeinfo".to_string(),
            mergeinfo.to_string(),
            ".".to_string(),
        ],
    )
}

fn apply_file_props(
    git: &GitCli,
    wc: &Path,
    commit: &str,
    path: &str,
    old_mode: &str,
) -> Result<(), String> {
    let new_mode = git.ls_tree_file(commit, path)?.mode;
    if new_mode == "100755" {
        run_svn(
            Some(wc),
            &[
                "propset".to_string(),
                "--non-interactive".to_string(),
                "svn:executable".to_string(),
                "x".to_string(),
                path.replace('\\', "/"),
            ],
        )?;
    } else if old_mode == "100755" {
        run_svn(
            Some(wc),
            &[
                "propdel".to_string(),
                "--non-interactive".to_string(),
                "svn:executable".to_string(),
                path.replace('\\', "/"),
            ],
        )?;
    }
    if new_mode == "120000" {
        run_svn(
            Some(wc),
            &[
                "propset".to_string(),
                "--non-interactive".to_string(),
                "svn:special".to_string(),
                "x".to_string(),
                path.replace('\\', "/"),
            ],
        )?;
    } else if old_mode == "120000" {
        run_svn(
            Some(wc),
            &[
                "propdel".to_string(),
                "--non-interactive".to_string(),
                "svn:special".to_string(),
                path.replace('\\', "/"),
            ],
        )?;
    }
    for (property, value) in svn_file_attributes_for_path(git, commit, path)? {
        run_svn(
            Some(wc),
            &[
                "propset".to_string(),
                "--non-interactive".to_string(),
                property,
                value,
                path.replace('\\', "/"),
            ],
        )?;
    }
    Ok(())
}

fn svn_file_attributes_for_path(
    git: &GitCli,
    commit: &str,
    path: &str,
) -> Result<Vec<(String, String)>, String> {
    let attributes = match git.show_file(commit, ".gitattributes") {
        Ok(attributes) => String::from_utf8(attributes).map_err(|e| e.to_string())?,
        Err(_) => return Ok(Vec::new()),
    };
    let mut svn_props = Vec::new();
    for line in attributes.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(pattern) = parts.next() else {
            continue;
        };
        if !attribute_pattern_matches(pattern, path) {
            continue;
        }
        for attr in parts {
            if let Some(value) = attr.strip_prefix("svn:eol-style=") {
                set_svn_file_attribute(&mut svn_props, "svn:eol-style", value);
            } else if let Some(value) = attr.strip_prefix("svn:mime-type=") {
                set_svn_file_attribute(&mut svn_props, "svn:mime-type", value);
            } else if let Some(value) = attr.strip_prefix("svn:keywords=") {
                set_svn_file_attribute(&mut svn_props, "svn:keywords", value);
            }
        }
    }
    Ok(svn_props)
}

fn set_svn_file_attribute(props: &mut Vec<(String, String)>, name: &str, value: &str) {
    if let Some((_, existing)) = props.iter_mut().find(|(property, _)| property == name) {
        *existing = value.to_string();
    } else {
        props.push((name.to_string(), value.to_string()));
    }
}

fn attribute_pattern_matches(pattern: &str, path: &str) -> bool {
    if pattern == path {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return path.ends_with(suffix);
    }
    if let Some((prefix, suffix)) = pattern.split_once('*') {
        let Some(rest) = path.strip_prefix(prefix) else {
            return false;
        };
        return rest.ends_with(suffix) && !rest.trim_end_matches(suffix).contains('/');
    }
    false
}

fn svn_commit(wc: &Path, message: &str) -> Result<u32, String> {
    let output = run_svn_output(
        Some(wc),
        &["commit".to_string(), "-m".to_string(), message.to_string()],
    )?;
    parse_committed_revision(&output)
}

fn parse_committed_revision(output: &str) -> Result<u32, String> {
    output
        .split(|c: char| !c.is_ascii_digit())
        .rfind(|part| !part.is_empty())
        .ok_or_else(|| format!("svn commit output did not include a revision: {output}"))?
        .parse()
        .map_err(|e| format!("invalid svn commit revision: {e}"))
}

fn svn_checkout_url(root_url: &str, svn_path: &str) -> String {
    if svn_path.is_empty() {
        root_url.trim_end_matches('/').to_string()
    } else {
        format!(
            "{}/{}",
            root_url.trim_end_matches('/'),
            svn_path.trim_matches('/')
        )
    }
}

fn run_svn(cwd: Option<&Path>, args: &[String]) -> Result<(), String> {
    run_svn_output(cwd, args).map(|_| ())
}

fn run_svn_output(cwd: Option<&Path>, args: &[String]) -> Result<String, String> {
    let mut command = Command::new("svn");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command
        .args(args)
        .output()
        .map_err(|e| format!("svn failed to start: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("svn exited with status {}", output.status)
        } else {
            stderr
        })
    }
}

struct TempCheckout {
    root: PathBuf,
    wc: PathBuf,
}

impl TempCheckout {
    fn new() -> Result<Self, String> {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "git-svn-rs-dcommit-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        let wc = root.join("wc");
        std::fs::create_dir_all(&wc).map_err(|e| e.to_string())?;
        Ok(Self { root, wc })
    }
}

impl Drop for TempCheckout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn dcommit_mock(
    git: &GitCli,
    refname: &str,
    mut rev_map: RevMap,
    uuid: &str,
    mut base_revision: u32,
    commits: Vec<GitCommitSummary>,
    no_rebase: bool,
) -> Result<String, String> {
    if commits.is_empty() {
        return Ok("No local commits to dcommit.\n".to_string());
    }

    let planner = GitDiffPlanner::new();
    let svn_editor = SvnCommitEditor::new(PropertyMapper);
    let mut backend = MockSvnBackend::new(uuid, Vec::new());
    let mut out = String::new();
    let mut diff_base = refname.to_string();

    for commit in &commits {
        let changes = git_changes_for_commit(git, &diff_base, commit)?;
        let planned = planner.plan(changes)?;
        let record = CommitRecord {
            author: git.commit_author(&commit.id)?,
            message: commit.subject.clone(),
            base_revision,
        };
        let revision = {
            let mut editor = backend.commit_editor(record);
            svn_editor.apply(&mut editor, planned.changes)?
        };
        rev_map.append(revision, &commit.id)?;
        git.update_ref(refname, &commit.id)?;
        base_revision = revision;
        diff_base = commit.id.clone();
        out.push_str(&format!(
            "Committed {} {} as r{revision}\n",
            commit.short_id, commit.subject
        ));
    }

    out.insert_str(
        0,
        &format!("Committed {} local Git commit(s)\n", commits.len()),
    );

    if no_rebase {
        out.push_str("Skipped rebase (--no-rebase).\n");
    } else {
        let rebase_output = git.rebase(refname, false, None)?;
        if rebase_output.is_empty() {
            out.push_str("Rebased onto tracked SVN ref.\n");
        } else {
            out.push_str(&rebase_output);
        }
    }

    Ok(out)
}

fn default_fetch_args() -> crate::cli::FetchArgs {
    crate::cli::FetchArgs {
        remote: None,
        shared: default_shared_args(),
        fetch_all: false,
        parent: false,
    }
}

fn default_shared_args() -> crate::cli::SharedFetchArgs {
    crate::cli::SharedFetchArgs {
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

fn git_changes_for_commit(
    git: &GitCli,
    base: &str,
    commit: &GitCommitSummary,
) -> Result<Vec<GitDiffChange>, String> {
    let mut changes = Vec::new();
    for change in git.diff_name_status(base, &commit.id)? {
        match change.status.as_str() {
            "A" => {
                let content = git.show_file(&commit.id, &change.path)?;
                let mode = git.ls_tree_file(&commit.id, &change.path)?.mode;
                changes.push(
                    GitDiffChange::add_file(change.path, content)
                        .with_executable(mode == "100755")
                        .with_symlink(mode == "120000"),
                );
            }
            "M" | "T" => {
                let content = git.show_file(&commit.id, &change.path)?;
                let mode = git.ls_tree_file(&commit.id, &change.path)?.mode;
                changes.push(
                    GitDiffChange::modify_file(change.path, content)
                        .with_executable(mode == "100755")
                        .with_symlink(mode == "120000"),
                );
            }
            "D" => changes.push(GitDiffChange::delete(change.path)),
            status if status.starts_with('R') => {
                let old_path = change
                    .old_path
                    .ok_or_else(|| format!("missing source path for {status}"))?;
                let content = git.show_file(&commit.id, &change.path)?;
                let mode = git.ls_tree_file(&commit.id, &change.path)?.mode;
                changes.push(GitDiffChange::delete(old_path));
                changes.push(
                    GitDiffChange::add_file(change.path, content)
                        .with_executable(mode == "100755")
                        .with_symlink(mode == "120000"),
                );
            }
            status if status.starts_with('C') => {
                let content = git.show_file(&commit.id, &change.path)?;
                let mode = git.ls_tree_file(&commit.id, &change.path)?.mode;
                changes.push(
                    GitDiffChange::add_file(change.path, content)
                        .with_executable(mode == "100755")
                        .with_symlink(mode == "120000"),
                );
            }
            status => {
                return Err(format!(
                    "dcommit does not support git diff status {status} yet"
                ));
            }
        }
    }
    Ok(changes)
}
