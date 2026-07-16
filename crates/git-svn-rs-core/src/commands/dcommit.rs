use crate::cli::DcommitArgs;
use crate::commands::resolver::resolve_tracked_svn;
use crate::commands::{fetch, rebase};
use crate::dcommit::journal_registry::{RepositoryDcommitLock, discover_repository_journals};
use crate::dcommit::{
    DcommitPlanBuilder, DcommitPlanRequest, DcommitTarget, PropertyMapper, SvnCommitEditor,
    merge_attribute_properties,
};
use crate::git::{GitCli, GitCommitSummary};
use crate::rev_map::RevMap;
use crate::svn::CommitRecord;
use crate::svn::mock::MockSvnBackend;
use std::path::{Path, PathBuf};
use std::process::Command;

mod working_copy;

use working_copy::WorkingCopyPlanEditor;

pub fn run(args: DcommitArgs) -> Result<String, String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: DcommitArgs,
) -> Result<String, String> {
    let tracked = resolve_tracked_svn(work_tree)?;
    if tracked.git.range_has_merges(&tracked.refname, "HEAD")? {
        return Err("dcommit does not support merge commits in the local commit range".to_string());
    }
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
        let svn_metadata_root = tracked
            .git
            .work_tree()
            .join(tracked.git.git_dir()?)
            .join("svn");
        let _repository_lock = RepositoryDcommitLock::acquire(&svn_metadata_root)
            .map_err(|error| error.to_string())?;
        if let Some(discovery) =
            discover_repository_journals(&svn_metadata_root).map_err(|error| error.to_string())?
        {
            if let Some(active) = discovery.active {
                return Err(format!(
                    "unfinished dcommit journal found at {}; automatic production recovery is not connected yet",
                    active.directory.display()
                ));
            }
            if let Some(completed) = discovery.completed.iter().find(|located| {
                located.journal.entries.iter().any(|entry| {
                    commits
                        .iter()
                        .any(|commit| commit.id.as_str() == entry.git_oid)
                })
            }) {
                return Err(format!(
                    "local commits overlap completed dcommit ledger at {}; rebase or reset before dcommit",
                    completed.directory.display()
                ));
            }
        }
        if target_url.starts_with("mock://") && tracked.config.url.starts_with("mock://") {
            if args.mergeinfo.is_some() {
                return Err(
                    "--mergeinfo write-back is only implemented for file:// URLs in v1".to_string(),
                );
            }
            return dcommit_mock(
                MockDcommit {
                    git: &tracked.git,
                    refname: &tracked.refname,
                    rev_map: tracked.open_rev_map()?,
                    uuid: &tracked.uuid,
                    target_url: &tracked.config.url,
                    base_revision: revision,
                    no_rebase: args.no_rebase,
                },
                commits,
            );
        }
        if is_svn_cli_write_back_url(target_url) && is_svn_cli_write_back_url(&tracked.config.url) {
            let commit_svn_path = if args.commit_url.is_some() {
                ""
            } else {
                &tracked.svn_path
            };
            let svn_options = dcommit_svn_options(
                tracked.config.username.as_deref(),
                tracked.config.config_dir.as_deref(),
                tracked.config.no_auth_cache,
                None,
                &args.shared,
            );
            return dcommit_file_svn(
                FileSvnDcommit {
                    git: &tracked.git,
                    svn_root_url: target_url,
                    svn_path: commit_svn_path,
                    uuid: &tracked.uuid,
                    refname: &tracked.refname,
                    base_revision: revision,
                    no_rebase: args.no_rebase,
                    mergeinfo: args.mergeinfo.as_deref(),
                    svn_options,
                    post_commit_fetch_shared: args.shared.clone(),
                },
                commits,
            );
        }
        {
            return Err(
                "dcommit write-back is only implemented for mock://, file://, and svn:// URLs; http(s) SVN write-back is not implemented"
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

fn is_svn_cli_write_back_url(url: &str) -> bool {
    url.starts_with("file://") || url.starts_with("svn://")
}

struct FileSvnDcommit<'a> {
    git: &'a GitCli,
    svn_root_url: &'a str,
    svn_path: &'a str,
    uuid: &'a str,
    refname: &'a str,
    base_revision: u32,
    no_rebase: bool,
    mergeinfo: Option<&'a str>,
    svn_options: DcommitSvnOptions,
    post_commit_fetch_shared: crate::cli::SharedFetchArgs,
}

fn dcommit_file_svn(
    ctx: FileSvnDcommit<'_>,
    commits: Vec<GitCommitSummary>,
) -> Result<String, String> {
    if commits.is_empty() {
        return Ok("No local commits to dcommit.\n".to_string());
    }

    let temp = TempCheckout::new()?;
    let checkout_url = svn_checkout_url(ctx.svn_root_url, ctx.svn_path);
    run_svn(
        Some(&temp.root),
        &ctx.svn_options,
        &[
            "checkout".to_string(),
            "--quiet".to_string(),
            checkout_url,
            "wc".to_string(),
        ],
    )?;

    let planner = DcommitPlanBuilder::new();
    let svn_editor = SvnCommitEditor::new(PropertyMapper);
    let target_url = svn_checkout_url(ctx.svn_root_url, ctx.svn_path);
    let mut base_revision = ctx.base_revision;
    let mut out = format!("Committed {} local Git commit(s)\n", commits.len());
    let mut diff_base = ctx.refname.to_string();
    for commit in &commits {
        let message = ctx.git.commit_message(&commit.id)?;
        let mut plan = planner.build(
            DcommitPlanRequest {
                target: DcommitTarget {
                    url: target_url.clone(),
                    repository_root: ctx.svn_root_url.to_string(),
                    repository_uuid: ctx.uuid.to_string(),
                    git_ref: ctx.refname.to_string(),
                },
                base_revision,
                git_commit: commit.id.clone(),
                message: message.clone(),
                author: Some(ctx.git.commit_author(&commit.id)?),
                mergeinfo: ctx.mergeinfo.map(str::to_owned),
                changes: ctx.git.diff_raw(&diff_base, &commit.id)?,
            },
            |path| ctx.git.show_file(&commit.id, path),
        )?;
        let base_attributes = git_attributes(ctx.git, &diff_base)?;
        let current_attributes = git_attributes(ctx.git, &commit.id)?;
        merge_attribute_properties(
            &mut plan,
            base_attributes.as_deref(),
            current_attributes.as_deref(),
        );
        let revision = {
            let mut editor =
                WorkingCopyPlanEditor::new(&temp.wc, &ctx.svn_options, message, base_revision);
            svn_editor.apply_plan(&mut editor, &plan)?
        };
        fetch::run_in_work_tree(
            ctx.git.work_tree().to_path_buf(),
            fetch_args(ctx.post_commit_fetch_shared.clone()),
        )?;
        base_revision = revision;
        diff_base = commit.id.clone();
        out.push_str(&format!(
            "Committed {} {} as r{revision}\n",
            commit.short_id, commit.subject
        ));
    }

    if ctx.no_rebase {
        out.push_str("Skipped rebase (--no-rebase).\n");
    } else {
        out.push_str(&rebase::run_in_work_tree(
            ctx.git.work_tree().to_path_buf(),
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

fn git_attributes(git: &GitCli, commit: &str) -> Result<Option<String>, String> {
    match git.show_file(commit, ".gitattributes") {
        Ok(content) => String::from_utf8(content)
            .map(Some)
            .map_err(|error| format!(".gitattributes is not valid UTF-8: {error}")),
        Err(_) => Ok(None),
    }
}

fn svn_commit(wc: &Path, message: &str, svn_options: &DcommitSvnOptions) -> Result<u32, String> {
    let output = run_svn_output(
        Some(wc),
        svn_options,
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

#[derive(Debug, Clone, Default)]
struct DcommitSvnOptions {
    config_dir: Option<String>,
    username: Option<String>,
    password: Option<String>,
    no_auth_cache: bool,
}

impl DcommitSvnOptions {
    fn command_args(&self, args: &[String]) -> Vec<String> {
        let mut command_args = Vec::new();
        if let Some(config_dir) = &self.config_dir {
            command_args.push("--config-dir".to_string());
            command_args.push(config_dir.clone());
        }
        if let Some(username) = &self.username {
            command_args.push("--username".to_string());
            command_args.push(username.clone());
        }
        if let Some(password) = &self.password {
            command_args.push("--password".to_string());
            command_args.push(password.clone());
        }
        if self.no_auth_cache {
            command_args.push("--no-auth-cache".to_string());
        }
        command_args.extend(args.iter().cloned());
        command_args
    }
}

fn dcommit_svn_options(
    persisted_username: Option<&str>,
    persisted_config_dir: Option<&str>,
    persisted_no_auth_cache: bool,
    persisted_password: Option<&str>,
    shared: &crate::cli::SharedFetchArgs,
) -> DcommitSvnOptions {
    DcommitSvnOptions {
        config_dir: shared
            .config_dir
            .clone()
            .or_else(|| persisted_config_dir.map(|value| value.to_string())),
        username: shared
            .username
            .clone()
            .or_else(|| persisted_username.map(|value| value.to_string())),
        password: shared
            .password
            .clone()
            .or_else(|| persisted_password.map(|value| value.to_string())),
        no_auth_cache: persisted_no_auth_cache || shared.no_auth_cache,
    }
}

fn run_svn(cwd: Option<&Path>, options: &DcommitSvnOptions, args: &[String]) -> Result<(), String> {
    run_svn_output(cwd, options, args).map(|_| ())
}

fn run_svn_output(
    cwd: Option<&Path>,
    options: &DcommitSvnOptions,
    args: &[String],
) -> Result<String, String> {
    let mut command = Command::new("svn");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let command_args = options.command_args(args);
    let output = command
        .args(command_args)
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
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let root = strip_windows_verbatim_prefix(root.canonicalize().map_err(|e| e.to_string())?);
        let wc = root.join("wc");
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

fn strip_windows_verbatim_prefix(path: PathBuf) -> PathBuf {
    let raw = path.to_string_lossy();
    if let Some(path) = raw.strip_prefix(r"\\?\") {
        PathBuf::from(path)
    } else {
        path
    }
}

struct MockDcommit<'a> {
    git: &'a GitCli,
    refname: &'a str,
    rev_map: RevMap,
    uuid: &'a str,
    target_url: &'a str,
    base_revision: u32,
    no_rebase: bool,
}

fn dcommit_mock(ctx: MockDcommit<'_>, commits: Vec<GitCommitSummary>) -> Result<String, String> {
    if commits.is_empty() {
        return Ok("No local commits to dcommit.\n".to_string());
    }

    let planner = DcommitPlanBuilder::new();
    let svn_editor = SvnCommitEditor::new(PropertyMapper);
    let mut backend = MockSvnBackend::new(ctx.uuid, Vec::new());
    let mut rev_map = ctx.rev_map;
    let mut base_revision = ctx.base_revision;
    let mut out = String::new();
    let mut diff_base = ctx.refname.to_string();

    for commit in &commits {
        let plan = planner.build(
            DcommitPlanRequest {
                target: DcommitTarget {
                    url: ctx.target_url.to_string(),
                    repository_root: ctx.target_url.to_string(),
                    repository_uuid: ctx.uuid.to_string(),
                    git_ref: ctx.refname.to_string(),
                },
                base_revision,
                git_commit: commit.id.clone(),
                message: ctx.git.commit_message(&commit.id)?,
                author: Some(ctx.git.commit_author(&commit.id)?),
                mergeinfo: None,
                changes: ctx.git.diff_raw(&diff_base, &commit.id)?,
            },
            |path| ctx.git.show_file(&commit.id, path),
        )?;
        let record = CommitRecord {
            author: plan.author.clone().unwrap_or_default(),
            message: plan.message.clone(),
            base_revision: plan.base_revision,
        };
        let revision = {
            let mut editor = backend.commit_editor(record);
            svn_editor.apply_plan(&mut editor, &plan)?
        };
        rev_map.append(revision, &commit.id)?;
        ctx.git.update_ref(ctx.refname, &commit.id)?;
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

    if ctx.no_rebase {
        out.push_str("Skipped rebase (--no-rebase).\n");
    } else {
        let rebase_output = ctx.git.rebase(ctx.refname, false, None)?;
        if rebase_output.is_empty() {
            out.push_str("Rebased onto tracked SVN ref.\n");
        } else {
            out.push_str(&rebase_output);
        }
    }

    Ok(out)
}

fn fetch_args(shared: crate::cli::SharedFetchArgs) -> crate::cli::FetchArgs {
    let mut shared = shared;
    shared.revision = None;
    crate::cli::FetchArgs {
        remote: None,
        shared,
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
        password: None,
        config_dir: None,
        no_auth_cache: false,
        preserve_empty_dirs: false,
        placeholder_filename: ".gitignore".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dcommit_svn_options_apply_command_line_auth_overrides() {
        let mut shared = default_shared_args();
        shared.username = Some("cli-user".to_string());
        shared.password = Some("cli-secret".to_string());
        shared.config_dir = Some("cli-config".to_string());
        shared.no_auth_cache = true;

        let options = dcommit_svn_options(
            Some("persisted-user"),
            Some("persisted-config"),
            false,
            Some("persisted-secret"),
            &shared,
        );

        assert_eq!(
            options.command_args(&["checkout".to_string(), "url".to_string()]),
            vec![
                "--config-dir",
                "cli-config",
                "--username",
                "cli-user",
                "--password",
                "cli-secret",
                "--no-auth-cache",
                "checkout",
                "url",
            ]
        );
    }
}
