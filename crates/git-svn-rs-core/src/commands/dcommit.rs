use crate::cli::DcommitArgs;
use crate::commands::resolver::resolve_tracked_svn;
use crate::commands::{fetch, rebase};
use crate::dcommit::journal_registry::{RepositoryDcommitLock, discover_repository_journals};
use crate::dcommit::{
    DcommitPlanBuilder, DcommitPlanRequest, DcommitTarget, PropertyMapper, SvnCommitEditor,
};
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
                    refname: &tracked.refname,
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
    refname: &'a str,
    no_rebase: bool,
    mergeinfo: Option<&'a str>,
    svn_options: DcommitSvnOptions,
    post_commit_fetch_shared: crate::cli::SharedFetchArgs,
}

struct FileSvnWorkingCopy<'a> {
    git: &'a GitCli,
    wc: &'a Path,
    svn_options: &'a DcommitSvnOptions,
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

    let mut out = format!("Committed {} local Git commit(s)\n", commits.len());
    let mut diff_base = ctx.refname.to_string();
    let wc = FileSvnWorkingCopy {
        git: ctx.git,
        wc: &temp.wc,
        svn_options: &ctx.svn_options,
    };
    for commit in &commits {
        let changes = ctx.git.diff_name_status(&diff_base, &commit.id)?;
        for change in changes {
            apply_file_svn_change(
                &wc,
                &diff_base,
                &commit.id,
                &change.status,
                &change.path,
                change.old_path.as_deref(),
            )?;
        }
        if let Some(mergeinfo) = ctx.mergeinfo {
            apply_mergeinfo(&temp.wc, mergeinfo, &ctx.svn_options)?;
        }
        let message = ctx.git.commit_message(&commit.id)?;
        let revision = svn_commit(&temp.wc, &message, &ctx.svn_options)?;
        fetch::run_in_work_tree(
            ctx.git.work_tree().to_path_buf(),
            fetch_args(ctx.post_commit_fetch_shared.clone()),
        )?;
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

fn apply_file_svn_change(
    wc: &FileSvnWorkingCopy<'_>,
    base: &str,
    commit: &str,
    status: &str,
    path: &str,
    old_path: Option<&str>,
) -> Result<(), String> {
    let target = wc.wc.join(path);
    match status {
        "A" => {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&target, svn_file_content(wc.git, commit, path)?)
                .map_err(|e| e.to_string())?;
            run_svn(
                Some(wc.wc),
                wc.svn_options,
                &[
                    "add".to_string(),
                    "--parents".to_string(),
                    path.replace('\\', "/"),
                ],
            )?;
            apply_file_props(wc.git, wc.wc, commit, path, "000000", None, wc.svn_options)?;
        }
        "M" | "T" => {
            std::fs::write(&target, svn_file_content(wc.git, commit, path)?)
                .map_err(|e| e.to_string())?;
            let old_mode = wc.git.ls_tree_file(base, path)?.mode;
            apply_file_props(
                wc.git,
                wc.wc,
                commit,
                path,
                &old_mode,
                Some((base, path)),
                wc.svn_options,
            )?;
        }
        "D" => {
            run_svn(
                Some(wc.wc),
                wc.svn_options,
                &["delete".to_string(), path.replace('\\', "/")],
            )?;
        }
        status if status.starts_with('R') => {
            let old_path = old_path.ok_or_else(|| format!("missing source path for {status}"))?;
            ensure_svn_parent_dirs(wc.wc, path, wc.svn_options)?;
            run_svn(
                Some(wc.wc),
                wc.svn_options,
                &[
                    "move".to_string(),
                    old_path.replace('\\', "/"),
                    path.replace('\\', "/"),
                ],
            )?;
            std::fs::write(&target, svn_file_content(wc.git, commit, path)?)
                .map_err(|e| e.to_string())?;
            let old_mode = wc.git.ls_tree_file(base, old_path)?.mode;
            apply_file_props(
                wc.git,
                wc.wc,
                commit,
                path,
                &old_mode,
                Some((base, old_path)),
                wc.svn_options,
            )?;
        }
        status if status.starts_with('C') => {
            let old_path = old_path.ok_or_else(|| format!("missing source path for {status}"))?;
            ensure_svn_parent_dirs(wc.wc, path, wc.svn_options)?;
            run_svn(
                Some(wc.wc),
                wc.svn_options,
                &[
                    "copy".to_string(),
                    old_path.replace('\\', "/"),
                    path.replace('\\', "/"),
                ],
            )?;
            std::fs::write(&target, svn_file_content(wc.git, commit, path)?)
                .map_err(|e| e.to_string())?;
            let old_mode = wc.git.ls_tree_file(base, old_path)?.mode;
            apply_file_props(
                wc.git,
                wc.wc,
                commit,
                path,
                &old_mode,
                Some((base, old_path)),
                wc.svn_options,
            )?;
        }
        other => {
            return Err(format!(
                "dcommit does not support git diff status {other} yet"
            ));
        }
    }
    Ok(())
}

fn ensure_svn_parent_dirs(
    wc: &Path,
    path: &str,
    svn_options: &DcommitSvnOptions,
) -> Result<(), String> {
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
        svn_options,
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

fn apply_mergeinfo(
    wc: &Path,
    mergeinfo: &str,
    svn_options: &DcommitSvnOptions,
) -> Result<(), String> {
    run_svn(
        Some(wc),
        svn_options,
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
    old_file: Option<(&str, &str)>,
    svn_options: &DcommitSvnOptions,
) -> Result<(), String> {
    let new_mode = git.ls_tree_file(commit, path)?.mode;
    if new_mode == "100755" {
        run_svn(
            Some(wc),
            svn_options,
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
            svn_options,
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
            svn_options,
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
            svn_options,
            &[
                "propdel".to_string(),
                "--non-interactive".to_string(),
                "svn:special".to_string(),
                path.replace('\\', "/"),
            ],
        )?;
    }
    let old_properties = match old_file {
        Some((old_commit, old_path)) => svn_file_attributes_for_path(git, old_commit, old_path)?,
        None => Vec::new(),
    };
    let new_properties = svn_file_attributes_for_path(git, commit, path)?;
    for (property, _) in &old_properties {
        if new_properties
            .iter()
            .all(|(new_property, _)| new_property != property)
        {
            run_svn(
                Some(wc),
                svn_options,
                &[
                    "propdel".to_string(),
                    "--non-interactive".to_string(),
                    property.clone(),
                    path.replace('\\', "/"),
                ],
            )?;
        }
    }
    for (property, value) in new_properties {
        run_svn(
            Some(wc),
            svn_options,
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
    let mut svn_properties = None;
    let mut property_operations = Vec::new();
    let mut attribute_order = 0;
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
            attribute_order += 1;
            if let Some(value) = attr.strip_prefix("svn-properties=") {
                svn_properties = Some((attribute_order, value));
            } else if attr == "-svn-properties" || attr == "!svn-properties" {
                svn_properties = None;
            } else if let Some(name) = direct_svn_property_clear(attr) {
                property_operations.push((attribute_order, name, None));
            } else if let Some(value) = attr.strip_prefix("svn:eol-style=") {
                property_operations.push((attribute_order, "svn:eol-style", Some(value)));
            } else if let Some(value) = attr.strip_prefix("svn:mime-type=") {
                property_operations.push((attribute_order, "svn:mime-type", Some(value)));
            } else if let Some(value) = attr.strip_prefix("svn:keywords=") {
                property_operations.push((attribute_order, "svn:keywords", Some(value)));
            } else if let Some(value) = attr.strip_prefix("svn:needs-lock=") {
                property_operations.push((attribute_order, "svn:needs-lock", Some(value)));
            } else if let Some(value) = attr.strip_prefix("svn:executable=") {
                property_operations.push((attribute_order, "svn:executable", Some(value)));
            } else if attr == "svn:executable" {
                property_operations.push((attribute_order, "svn:executable", Some("x")));
            } else if let Some(value) = attr.strip_prefix("svn:special=") {
                property_operations.push((attribute_order, "svn:special", Some(value)));
            } else if attr == "svn:special" {
                property_operations.push((attribute_order, "svn:special", Some("x")));
            } else if attr == "svn:needs-lock" {
                property_operations.push((attribute_order, "svn:needs-lock", Some("x")));
            }
        }
    }
    if let Some((order, value)) = svn_properties {
        for property in value.split(';').filter(|property| !property.is_empty()) {
            if let Some((name, value)) = property.split_once('=')
                && !name.is_empty()
                && !value.is_empty()
            {
                property_operations.push((order, name, Some(value)));
            }
        }
    }
    property_operations.sort_by_key(|(order, _, _)| *order);
    let mut svn_props = Vec::new();
    for (_, name, value) in property_operations {
        apply_svn_file_attribute_operation(&mut svn_props, name, value);
    }
    Ok(svn_props)
}

fn direct_svn_property_clear(attr: &str) -> Option<&'static str> {
    match attr {
        "-svn:eol-style" | "!svn:eol-style" => Some("svn:eol-style"),
        "-svn:mime-type" | "!svn:mime-type" => Some("svn:mime-type"),
        "-svn:keywords" | "!svn:keywords" => Some("svn:keywords"),
        "-svn:executable" | "!svn:executable" => Some("svn:executable"),
        "-svn:special" | "!svn:special" => Some("svn:special"),
        "-svn:needs-lock" | "!svn:needs-lock" => Some("svn:needs-lock"),
        _ => None,
    }
}

fn apply_svn_file_attribute_operation(
    props: &mut Vec<(String, String)>,
    name: &str,
    value: Option<&str>,
) {
    if let Some(value) = value {
        if let Some((_, existing)) = props.iter_mut().find(|(property, _)| property == name) {
            *existing = value.to_string();
        } else {
            props.push((name.to_string(), value.to_string()));
        }
    } else if let Some(index) = props.iter().position(|(property, _)| property == name) {
        props.remove(index);
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
