use crate::cli::DcommitArgs;
use crate::commands::resolver::{
    resolve_tracked_svn_allow_import_batch_typed, resolve_tracked_svn_path,
    resolve_tracked_svn_typed,
};
use crate::commands::{fetch, rebase};
use crate::dcommit::coordinator::{CommitSink, Coordinator, PostSubmit, RemoteHead};
use crate::dcommit::journal::{
    DcommitJournal, DcommitTargetIdentity, EntryState, JournalEntry, JournalStore,
};
use crate::dcommit::journal_persistence::JournalStorePersistence;
use crate::dcommit::journal_registry::{
    JournalDiscovery, LocatedJournal, RepositoryDcommitLock, discover_repository_journals,
};
use crate::dcommit::tree_projection::{apply_plan_to_tree, canonicalize_tree_keywords, tree_map};
use crate::dcommit::{
    DcommitPlan, DcommitPlanBuilder, DcommitPlanRequest, DcommitTarget, PreparedDcommitRequest,
    PropertyMapper, RecoveryFetchIntent, RecoveryFingerprintInput, SvnCommitEditor,
    build_prepared_dcommit, merge_attribute_properties, recovery_config_fingerprint,
};
use crate::error::GitSvnError;
use crate::git::{GitCli, GitCommitSummary};
use crate::git_svn_id::GitSvnId;
use crate::rev_map::RevMap;
use crate::svn::CommitRecord;
use crate::svn::auth::{AuthOperation, prompted_credentials};
use crate::svn::mock::MockSvnBackend;
use std::path::{Path, PathBuf};
use std::process::Command;

mod planning;
mod post_submit;
mod preflight;
mod target;
mod working_copy;

use planning::*;
use post_submit::*;
use preflight::*;
use target::*;
use working_copy::*;

pub fn run(args: DcommitArgs) -> Result<String, String> {
    run_typed(args).map_err(|error| error.to_string())
}

pub fn run_typed(args: DcommitArgs) -> Result<String, GitSvnError> {
    run_in_work_tree_typed(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: DcommitArgs,
) -> Result<String, String> {
    run_in_work_tree_typed(work_tree, args).map_err(|error| error.to_string())
}

fn run_in_work_tree_typed(
    work_tree: impl Into<std::path::PathBuf>,
    args: DcommitArgs,
) -> Result<String, GitSvnError> {
    if args.shared.revision.is_some() {
        return Err(GitSvnError::invalid_invocation(
            "dcommit --revision is not supported in v1; refusing to ignore an SVN editor base override"
                .to_string(),
        ));
    }
    let work_tree = work_tree.into();
    let git = GitCli::new(&work_tree);
    if !args.dry_run && crate::import_transaction::has_pending_batch(&git)? {
        let pending = resolve_tracked_svn_allow_import_batch_typed(&work_tree)?;
        reject_read_mirror_dcommit(&pending.config, true)?;
        crate::path_url::validate_fetch_url(&pending.config.url)?;
        let refnames = crate::import_transaction::pending_batch_refnames(&git)?;
        for refname in refnames {
            fetch::run_for_tracking_identity(
                &work_tree,
                pending.config.clone(),
                &refname,
                &args.shared,
            )?;
        }
    }
    let tracked = resolve_tracked_svn_typed(work_tree)?;
    if tracked.config.no_metadata {
        return Err(GitSvnError::unsupported(
            "dcommit is unavailable for --no-metadata one-shot imports",
        ));
    }
    reject_read_mirror_dcommit(&tracked.config, false)?;
    if tracked.git.range_has_merges(&tracked.refname, "HEAD")? {
        return Err(GitSvnError::unsupported(
            "dcommit does not support merge commits in the local commit range",
        ));
    }
    let revision = tracked
        .max_record_typed()?
        .map(|record| record.revision)
        .unwrap_or(0);
    let target = resolve_dcommit_target(&tracked, args.commit_url.as_deref())?;
    let target_url = target.commit_url.as_str();
    if !args.dry_run {
        if crate::path_url::svn_url_profile(target_url) == crate::path_url::SvnUrlProfile::SvnSsh
            && args
                .shared
                .config_dir
                .as_ref()
                .or(tracked.config.config_dir.as_ref())
                .is_none()
        {
            return Err(GitSvnError::invalid_invocation(
                "svn+ssh dcommit requires --config-dir or svn-remote.<name>.config-dir for a configured non-interactive tunnel"
                    .to_string(),
            ));
        }
        crate::path_url::validate_dcommit_write_urls(target_url, &tracked.config.url)?;
    }
    crate::tracking_state::validate_existing_tracking_state(
        &tracked.git,
        &tracked.config,
        &tracked.refname,
        &tracked.svn_path,
        &tracked.uuid,
        &tracked.rev_map_path,
    )?;
    let commits = if tracked.git.rev_parse("HEAD").is_ok() {
        tracked
            .git
            .commit_summaries_between(&tracked.refname, "HEAD")?
    } else {
        Vec::new()
    };
    let svn_metadata_root = tracked
        .git
        .work_tree()
        .join(tracked.git.git_dir()?)
        .join("svn");

    if !args.dry_run {
        let _repository_lock = RepositoryDcommitLock::acquire(&svn_metadata_root)
            .map_err(|error| error.to_string())?;
        let discovery =
            discover_repository_journals(&svn_metadata_root).map_err(|error| error.to_string())?;
        if args.adopt_revision.is_some()
            && discovery
                .as_ref()
                .and_then(|value| value.active.as_ref())
                .is_none()
        {
            return Err(GitSvnError::invalid_invocation(
                "--adopt-revision requires an unfinished dcommit journal with an in-flight submission"
                    .to_string(),
            ));
        }
        let may_submit = discovery
            .as_ref()
            .and_then(|value| value.active.as_ref())
            .is_none_or(|active| {
                active.journal.entries.iter().any(|entry| {
                    matches!(entry.state, EntryState::Queued | EntryState::Ready { .. })
                })
            });
        if may_submit && !tracked.git.is_work_tree_clean()? {
            return Err(GitSvnError::invalid_invocation(
                "dcommit requires a clean index and working tree before SVN write-back".to_string(),
            ));
        }
        reject_completed_ledger_overlap(discovery.as_ref(), &commits)?;
        if target_url.starts_with("mock://") && tracked.config.url.starts_with("mock://") {
            if args.adopt_revision.is_some() {
                return Err(GitSvnError::unsupported(
                    "--adopt-revision is only implemented for real file:// and svn:// dcommit recovery"
                        .to_string(),
                ));
            }
            if let Some(active) = discovery.as_ref().and_then(|value| value.active.as_ref()) {
                return Err(GitSvnError::metadata_corruption(format!(
                    "unfinished dcommit journal found at {}; mock recovery is not implemented",
                    active.directory.display()
                )));
            }
            if args.mergeinfo.is_some() {
                return Err(GitSvnError::unsupported(
                    "--mergeinfo write-back is only implemented for file:// URLs in v1".to_string(),
                ));
            }
            return dcommit_mock(
                MockDcommit {
                    git: &tracked.git,
                    refname: &tracked.refname,
                    rev_map: tracked.open_rev_map_typed()?,
                    uuid: &tracked.uuid,
                    target_url: &tracked.config.url,
                    base_revision: revision,
                    no_rebase: args.no_rebase,
                },
                commits,
            )
            .map_err(GitSvnError::from);
        }
        if is_svn_cli_write_back_url(target_url) && is_svn_cli_write_back_url(&tracked.config.url) {
            let svn_options = resolved_dcommit_svn_options(&tracked, &args, target_url)?;
            let post_commit_fetch_config =
                fetch::effective_fetch_config(tracked.config.clone(), &args.shared, revision)?;
            return dcommit_file_svn(
                FileSvnDcommit {
                    git: &tracked.git,
                    svn_root_url: &target.repository_root,
                    svn_path: &target.svn_path,
                    uuid: &tracked.uuid,
                    source_refname: &tracked.refname,
                    mapping_ref: &target.mapping_ref,
                    no_rebase: args.no_rebase,
                    mergeinfo: args.mergeinfo.as_deref(),
                    svn_options,
                    post_commit_fetch_shared: args.shared.clone(),
                    post_commit_fetch_config,
                    remote_id: &tracked.config.name,
                    rev_map_path: &target.rev_map_path,
                    expected_footer_url: svn_checkout_url(
                        tracked
                            .config
                            .rewrite_root
                            .as_deref()
                            .unwrap_or(&tracked.config.url),
                        &target.mapping_svn_path,
                    ),
                    expected_footer_uuid: tracked
                        .config
                        .rewrite_uuid
                        .clone()
                        .unwrap_or_else(|| tracked.uuid.clone()),
                    commit_url_override: target.commit_url_override,
                    adopt_revision: args.adopt_revision,
                },
                commits,
                discovery.and_then(|value| value.active),
            );
        }
        {
            return Err(GitSvnError::unsupported(
                "dcommit write-back URL profile is unsupported",
            ));
        }
    }

    let discovery =
        discover_repository_journals(&svn_metadata_root).map_err(|error| error.to_string())?;
    if let Some(active) = discovery.as_ref().and_then(|value| value.active.as_ref()) {
        return Err(GitSvnError::metadata_corruption(format!(
            "unfinished dcommit journal found at {}; dcommit --dry-run is read-only and will not recover it",
            active.directory.display()
        )));
    }
    reject_completed_ledger_overlap(discovery.as_ref(), &commits)?;
    dcommit_dry_run(&tracked, &args, &target, &commits).map_err(GitSvnError::from)
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

    let svn_editor = SvnCommitEditor::new(PropertyMapper);
    let plans = build_dcommit_plans(
        &DcommitPlanningContext {
            git: ctx.git,
            repository_root: ctx.target_url,
            repository_uuid: ctx.uuid,
            mapping_ref: ctx.refname,
            mergeinfo: None,
        },
        ctx.target_url,
        u64::from(ctx.base_revision),
        &new_plan_chain(ctx.refname, &commits),
        true,
    )?;
    if plans.is_empty() {
        return finish_noop_dcommit(ctx.git, ctx.refname, ctx.no_rebase);
    }

    let mut backend = MockSvnBackend::new(ctx.uuid, Vec::new());
    let mut rev_map = ctx.rev_map;
    let mut out = String::new();
    for plan in &plans {
        let commit = commits
            .iter()
            .find(|commit| commit.id == plan.git_commit)
            .ok_or_else(|| {
                format!(
                    "dcommit plan references commit outside the local queue: {}",
                    plan.git_commit
                )
            })?;
        let record = CommitRecord {
            author: plan.author.clone().unwrap_or_default(),
            message: plan.message.clone(),
            base_revision: plan.base_revision,
        };
        let revision = {
            let mut editor = backend.commit_editor(record);
            svn_editor.apply_plan(&mut editor, plan)?
        };
        rev_map.append(revision, &commit.id)?;
        ctx.git.update_ref(ctx.refname, &commit.id)?;
        out.push_str(&format!(
            "Committed {} {} as r{revision}\n",
            commit.short_id, commit.subject
        ));
    }

    out.insert_str(
        0,
        &format!("Committed {} local Git commit(s)\n", plans.len()),
    );

    if ctx.no_rebase {
        out.push_str("Skipped rebase (--no-rebase).\n");
    } else {
        let rebase_output = ctx.git.rebase(ctx.refname, false, false, None, false)?;
        if rebase_output.is_empty() {
            out.push_str("Rebased onto tracked SVN ref.\n");
        } else {
            out.push_str(&rebase_output);
        }
    }

    Ok(out)
}

#[cfg(test)]
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
        use_svm_props: false,
        use_svnsync_props: false,
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
    fn svm_dcommit_rejects_normal_and_pending_import_paths() {
        let config = crate::config::SvnRemoteConfig::new(
            "svn",
            "mock://repo",
            crate::mapping::build_single_path(""),
        )
        .with_svm_props();

        let pending = reject_read_mirror_dcommit(&config, true).unwrap_err();
        assert!(pending.contains("useSvmProps"));
        assert!(pending.contains("before rejecting mirror write-back"));
        let normal = reject_read_mirror_dcommit(&config, false).unwrap_err();
        assert!(normal.contains("useSvmProps"));
        assert!(normal.contains("refusing to write through"));
    }

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
                "--non-interactive",
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

    #[test]
    fn dcommit_svn_options_apply_persisted_non_secret_auth_defaults() {
        let options = dcommit_svn_options(
            Some("persisted-user"),
            Some("persisted-config"),
            true,
            None,
            &default_shared_args(),
        );

        assert_eq!(
            options.command_args(&["info".to_string(), "url".to_string()]),
            vec![
                "--non-interactive",
                "--config-dir",
                "persisted-config",
                "--username",
                "persisted-user",
                "--no-auth-cache",
                "info",
                "url",
            ]
        );
    }

    #[test]
    fn recovery_config_fingerprint_accepts_current_or_migrates_v2_and_v3() {
        let mut current = "v4".to_string();
        reconcile_recovery_config_fingerprint(&mut current, "v4", &["v3", "v2"]).unwrap();
        assert_eq!(current, "v4");

        let mut legacy = "v2".to_string();
        reconcile_recovery_config_fingerprint(&mut legacy, "v4", &["v3", "v2"]).unwrap();
        assert_eq!(legacy, "v4");

        let mut legacy = "v3".to_string();
        reconcile_recovery_config_fingerprint(&mut legacy, "v4", &["v3", "v2"]).unwrap();
        assert_eq!(legacy, "v4");

        let mut mismatch = "other".to_string();
        assert!(
            reconcile_recovery_config_fingerprint(&mut mismatch, "v4", &["v3", "v2"])
                .unwrap_err()
                .contains("does not match")
        );
        assert_eq!(mismatch, "other");
    }

    #[test]
    fn dcommit_message_preserves_body_and_removes_git_format_terminators() {
        assert_eq!(
            dcommit_message("subject\n\nbody with trailing spaces  \n\n"),
            "subject\n\nbody with trailing spaces  "
        );
        assert_eq!(
            dcommit_message("subject\r\n\r\nbody\r\n"),
            "subject\r\n\r\nbody"
        );
    }
}
