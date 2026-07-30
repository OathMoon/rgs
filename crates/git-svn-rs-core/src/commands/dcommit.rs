use crate::cli::DcommitArgs;
use crate::commands::resolver::{
    resolve_tracked_svn, resolve_tracked_svn_allow_import_batch, resolve_tracked_svn_path,
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
use crate::git::{GitCli, GitCommitSummary};
use crate::git_svn_id::GitSvnId;
use crate::rev_map::RevMap;
use crate::svn::CommitRecord;
use crate::svn::auth::askpass_password;
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
    if args.shared.revision.is_some() {
        return Err(
            "dcommit --revision is not supported in v1; refusing to ignore an SVN editor base override"
                .to_string(),
        );
    }
    let work_tree = work_tree.into();
    let git = GitCli::new(&work_tree);
    if !args.dry_run && crate::import_transaction::has_pending_batch(&git)? {
        let pending = resolve_tracked_svn_allow_import_batch(&work_tree)?;
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
    let tracked = resolve_tracked_svn(work_tree)?;
    if tracked.config.no_metadata {
        return Err("dcommit is unavailable for --no-metadata one-shot imports".to_string());
    }
    if tracked.git.range_has_merges(&tracked.refname, "HEAD")? {
        return Err("dcommit does not support merge commits in the local commit range".to_string());
    }
    let revision = tracked
        .max_record()?
        .map(|record| record.revision)
        .unwrap_or(0);
    let target_url = args.commit_url.as_deref().unwrap_or(&tracked.config.url);
    if !args.dry_run && args.commit_url.is_some() && tracked.config.url.starts_with("mock://") {
        return Err(
            "--commit-url is not supported for mock:// dcommit write-back in v1".to_string(),
        );
    }
    if !args.dry_run {
        if crate::path_url::svn_url_profile(target_url) == crate::path_url::SvnUrlProfile::SvnSsh
            && args
                .shared
                .config_dir
                .as_ref()
                .or(tracked.config.config_dir.as_ref())
                .is_none()
        {
            return Err(
                "svn+ssh dcommit requires --config-dir or svn-remote.<name>.config-dir for a configured non-interactive tunnel"
                    .to_string(),
            );
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
            return Err(
                "--adopt-revision requires an unfinished dcommit journal with an in-flight submission"
                    .to_string(),
            );
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
            return Err(
                "dcommit requires a clean index and working tree before SVN write-back".to_string(),
            );
        }
        reject_completed_ledger_overlap(discovery.as_ref(), &commits)?;
        if target_url.starts_with("mock://") && tracked.config.url.starts_with("mock://") {
            if args.adopt_revision.is_some() {
                return Err(
                    "--adopt-revision is only implemented for real file:// and svn:// dcommit recovery"
                        .to_string(),
                );
            }
            if let Some(active) = discovery.as_ref().and_then(|value| value.active.as_ref()) {
                return Err(format!(
                    "unfinished dcommit journal found at {}; mock recovery is not implemented",
                    active.directory.display()
                ));
            }
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
            let commit_mapping = if args.commit_url.is_some() {
                let svn_path = commit_url_path(&tracked.config.url, target_url)?;
                Some(resolve_tracked_svn_path(&tracked, &svn_path)?)
            } else {
                None
            };
            let mapping_ref = commit_mapping
                .as_ref()
                .map_or(tracked.refname.as_str(), |mapping| mapping.refname.as_str());
            let mapping_svn_path = commit_mapping
                .as_ref()
                .map_or(tracked.svn_path.as_str(), |mapping| {
                    mapping.svn_path.as_str()
                });
            let mapping_rev_map_path = commit_mapping
                .as_ref()
                .map_or(tracked.rev_map_path.as_path(), |mapping| {
                    mapping.rev_map_path.as_path()
                });
            let mut svn_options = dcommit_svn_options(
                tracked.config.username.as_deref(),
                tracked.config.config_dir.as_deref(),
                tracked.config.no_auth_cache,
                None,
                &args.shared,
            );
            if svn_options.password.is_none()
                && crate::path_url::svn_url_profile(target_url)
                    == crate::path_url::SvnUrlProfile::Svn
            {
                svn_options.password = askpass_password(
                    target_url,
                    svn_options.username.as_deref(),
                    svn_options.no_auth_cache,
                )?;
            }
            if target_url.starts_with("file://") {
                svn_options.username = None;
                svn_options.password = None;
            }
            let post_commit_fetch_config =
                fetch::effective_fetch_config(tracked.config.clone(), &args.shared, revision)?;
            return dcommit_file_svn(
                FileSvnDcommit {
                    git: &tracked.git,
                    svn_root_url: target_url,
                    svn_path: commit_svn_path,
                    uuid: &tracked.uuid,
                    source_refname: &tracked.refname,
                    mapping_ref,
                    no_rebase: args.no_rebase,
                    mergeinfo: args.mergeinfo.as_deref(),
                    svn_options,
                    post_commit_fetch_shared: args.shared.clone(),
                    post_commit_fetch_config,
                    remote_id: &tracked.config.name,
                    rev_map_path: mapping_rev_map_path,
                    expected_footer_url: svn_checkout_url(
                        tracked
                            .config
                            .rewrite_root
                            .as_deref()
                            .unwrap_or(&tracked.config.url),
                        mapping_svn_path,
                    ),
                    expected_footer_uuid: tracked
                        .config
                        .rewrite_uuid
                        .clone()
                        .unwrap_or_else(|| tracked.uuid.clone()),
                    commit_url_override: args.commit_url.is_some(),
                    adopt_revision: args.adopt_revision,
                },
                commits,
                discovery.and_then(|value| value.active),
            );
        }
        {
            return Err(
                "dcommit write-back is only implemented for mock://, file://, svn://, and configured svn+ssh:// URLs; HTTP(S) write-back is not implemented"
                    .to_string(),
            );
        }
    }

    let discovery =
        discover_repository_journals(&svn_metadata_root).map_err(|error| error.to_string())?;
    if let Some(active) = discovery.as_ref().and_then(|value| value.active.as_ref()) {
        return Err(format!(
            "unfinished dcommit journal found at {}; dcommit --dry-run is read-only and will not recover it",
            active.directory.display()
        ));
    }
    reject_completed_ledger_overlap(discovery.as_ref(), &commits)?;
    dcommit_dry_run(&tracked, &args, &commits)
}

fn reject_completed_ledger_overlap(
    discovery: Option<&JournalDiscovery>,
    commits: &[GitCommitSummary],
) -> Result<(), String> {
    if let Some(completed) = discovery.and_then(|discovery| {
        discovery.completed.iter().find(|located| {
            located.journal.entries.iter().any(|entry| {
                commits
                    .iter()
                    .any(|commit| commit.id.as_str() == entry.git_oid)
            })
        })
    }) {
        return Err(format!(
            "local commits overlap completed dcommit ledger at {}; rebase or reset before dcommit",
            completed.directory.display()
        ));
    }
    Ok(())
}

struct DcommitPlanningContext<'a> {
    git: &'a GitCli,
    repository_root: &'a str,
    repository_uuid: &'a str,
    mapping_ref: &'a str,
    mergeinfo: Option<&'a str>,
}

struct DryRunTarget {
    repository_root: String,
    commit_url: String,
    mapping_ref: String,
    rev_map_path: PathBuf,
}

fn dcommit_dry_run(
    tracked: &crate::commands::resolver::TrackedSvn,
    args: &DcommitArgs,
    commits: &[GitCommitSummary],
) -> Result<String, String> {
    let target = resolve_dry_run_target(tracked, args.commit_url.as_deref())?;
    let (base_revision, _) =
        validate_tracking_base(&tracked.git, &target.mapping_ref, &target.rev_map_path)?;
    let chain = new_plan_chain(&tracked.refname, commits);
    let plans = build_dcommit_plans(
        &DcommitPlanningContext {
            git: &tracked.git,
            repository_root: &target.repository_root,
            repository_uuid: &tracked.uuid,
            mapping_ref: &target.mapping_ref,
            mergeinfo: args.mergeinfo.as_deref(),
        },
        &target.commit_url,
        base_revision,
        &chain,
        true,
    )?;

    let mut out = format!(
        "Dcommit dry-run against {} ({}, r{base_revision})\n",
        target.commit_url, target.mapping_ref
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
    if plans.is_empty() {
        out.push_str("No changes to dcommit.\n");
        return Ok(out);
    }

    out.push_str(&format!(
        "Would commit {} local Git commit(s):\n",
        plans.len()
    ));
    for plan in plans {
        let commit = commits
            .iter()
            .find(|commit| commit.id == plan.git_commit)
            .ok_or_else(|| {
                format!(
                    "dcommit plan references commit outside the local queue: {}",
                    plan.git_commit
                )
            })?;
        out.push_str(&format!("{} {}\n", commit.short_id, commit.subject));
    }
    Ok(out)
}

fn resolve_dry_run_target(
    tracked: &crate::commands::resolver::TrackedSvn,
    commit_url: Option<&str>,
) -> Result<DryRunTarget, String> {
    if let Some(commit_url) = commit_url {
        if tracked.config.url.starts_with("mock://") {
            return Err(
                "--commit-url is not supported for mock:// dcommit write-back in v1".to_string(),
            );
        }
        let svn_path = commit_url_path(&tracked.config.url, commit_url)?;
        let mapping = resolve_tracked_svn_path(tracked, &svn_path)?;
        return Ok(DryRunTarget {
            repository_root: commit_url.to_string(),
            commit_url: commit_url.to_string(),
            mapping_ref: mapping.refname,
            rev_map_path: mapping.rev_map_path,
        });
    }
    Ok(DryRunTarget {
        repository_root: tracked.config.url.clone(),
        commit_url: svn_checkout_url(&tracked.config.url, &tracked.svn_path),
        mapping_ref: tracked.refname.clone(),
        rev_map_path: tracked.rev_map_path.clone(),
    })
}

fn is_svn_cli_write_back_url(url: &str) -> bool {
    matches!(
        crate::path_url::svn_url_profile(url),
        crate::path_url::SvnUrlProfile::File
            | crate::path_url::SvnUrlProfile::Svn
            | crate::path_url::SvnUrlProfile::SvnSsh
    )
}

fn dcommit_message(message: &str) -> String {
    message.trim_end_matches(['\r', '\n']).to_string()
}

fn commit_url_path(remote_url: &str, commit_url: &str) -> Result<String, String> {
    let remote_url = crate::path_url::canonicalize_url(remote_url);
    let commit_url = crate::path_url::canonicalize_url(commit_url);
    if commit_url == remote_url {
        return Ok(String::new());
    }
    commit_url
        .strip_prefix(&format!("{remote_url}/"))
        .map(str::to_owned)
        .ok_or_else(|| {
            format!(
                "commit URL {commit_url} is outside the configured SVN remote {remote_url}; refusing before write setup"
            )
        })
}

struct FileSvnDcommit<'a> {
    git: &'a GitCli,
    svn_root_url: &'a str,
    svn_path: &'a str,
    uuid: &'a str,
    source_refname: &'a str,
    mapping_ref: &'a str,
    no_rebase: bool,
    mergeinfo: Option<&'a str>,
    svn_options: DcommitSvnOptions,
    post_commit_fetch_shared: crate::cli::SharedFetchArgs,
    post_commit_fetch_config: crate::config::SvnRemoteConfig,
    remote_id: &'a str,
    rev_map_path: &'a Path,
    expected_footer_url: String,
    expected_footer_uuid: String,
    commit_url_override: bool,
    adopt_revision: Option<u64>,
}

fn dcommit_file_svn(
    ctx: FileSvnDcommit<'_>,
    commits: Vec<GitCommitSummary>,
    active: Option<LocatedJournal>,
) -> Result<String, String> {
    if commits.is_empty() && active.is_none() {
        return Ok("No local commits to dcommit.\n".to_string());
    }

    let target_url = svn_checkout_url(ctx.svn_root_url, ctx.svn_path);
    validate_svn_repository_uuid(&target_url, ctx.uuid, &ctx.svn_options)?;
    let target = DcommitTargetIdentity {
        remote_id: ctx.remote_id.to_string(),
        repository_root_url: ctx.svn_root_url.to_string(),
        repository_uuid: ctx.uuid.to_string(),
        mapping_ref: ctx.mapping_ref.to_string(),
        rev_map_path: ctx.rev_map_path.to_string_lossy().into_owned(),
        commit_url: target_url.clone(),
    };
    let authors_file_bytes = ctx
        .post_commit_fetch_config
        .authors_file
        .as_deref()
        .map(|path| {
            std::fs::read(path)
                .map_err(|error| format!("failed to read authors file {path}: {error}"))
        })
        .transpose()?;
    let fetch_intent = RecoveryFetchIntent {
        authors_file: ctx
            .post_commit_fetch_config
            .authors_file
            .as_deref()
            .map(Path::new),
        authors_file_bytes: authors_file_bytes.as_deref(),
        authors_prog: ctx.post_commit_fetch_config.authors_prog.as_deref(),
        ignore_paths: ctx.post_commit_fetch_config.ignore_paths.as_deref(),
        include_paths: ctx.post_commit_fetch_config.include_paths.as_deref(),
        ignore_refs: ctx.post_commit_fetch_config.ignore_refs.as_deref(),
        localtime: ctx.post_commit_fetch_config.localtime,
        no_metadata: ctx.post_commit_fetch_config.no_metadata,
        rewrite_root: ctx.post_commit_fetch_config.rewrite_root.as_deref(),
        rewrite_uuid: ctx.post_commit_fetch_config.rewrite_uuid.as_deref(),
        preserve_empty_dirs: ctx.post_commit_fetch_config.preserve_empty_dirs,
        placeholder_filename: ctx
            .post_commit_fetch_config
            .preserve_empty_dirs
            .then_some(ctx.post_commit_fetch_config.placeholder_filename.as_str()),
    };
    let recovery_fingerprint_input = RecoveryFingerprintInput {
        target: &target,
        commit_url_override: ctx.commit_url_override,
        username: ctx.svn_options.username.as_deref(),
        config_dir: ctx.svn_options.config_dir.as_deref().map(Path::new),
        no_auth_cache: ctx.svn_options.no_auth_cache,
        no_rebase: ctx.no_rebase,
        mergeinfo: ctx.mergeinfo,
        fetch: fetch_intent,
    };
    let config_fingerprint = recovery_config_fingerprint(recovery_fingerprint_input);
    let legacy_config_fingerprint_v2 =
        crate::dcommit::fingerprint::legacy_recovery_config_fingerprint_v2(
            recovery_fingerprint_input,
        );
    let legacy_config_fingerprint_v3 =
        crate::dcommit::fingerprint::legacy_recovery_config_fingerprint_v3(
            recovery_fingerprint_input,
        );
    let new_base = active
        .is_none()
        .then(|| {
            validate_new_dcommit_base(
                ctx.git,
                ctx.mapping_ref,
                ctx.rev_map_path,
                &target_url,
                &ctx.svn_options,
            )
        })
        .transpose()?;
    let plan_chain = if let Some(located) = &active {
        located
            .journal
            .entries
            .iter()
            .map(|entry| {
                ctx.git
                    .rev_parse(&format!("{}^", entry.git_oid))
                    .map(|base| (base.trim().to_string(), entry.git_oid.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        new_plan_chain(ctx.source_refname, &commits)
    };
    let original_base_revision = match &active {
        Some(located) => located.journal.original_base_revision,
        None => {
            new_base
                .as_ref()
                .expect("new dcommit base must be validated")
                .0
        }
    };
    let original_base_oid = match &active {
        Some(located) => located.journal.original_base_oid.clone(),
        None => new_base.expect("new dcommit base must be validated").1,
    };
    let plans = build_dcommit_plans(
        &DcommitPlanningContext {
            git: ctx.git,
            repository_root: ctx.svn_root_url,
            repository_uuid: ctx.uuid,
            mapping_ref: ctx.mapping_ref,
            mergeinfo: ctx.mergeinfo,
        },
        &target_url,
        original_base_revision,
        &plan_chain,
        active.is_none(),
    )?;
    if plans.is_empty() && active.is_none() {
        return finish_noop_dcommit(ctx.git, ctx.mapping_ref, ctx.no_rebase);
    }
    let original_head = match &active {
        Some(located) => located.journal.original_head.clone(),
        None => plans
            .last()
            .expect("new dcommit plan queue must be nonempty")
            .git_commit
            .clone(),
    };
    let mut prepared = build_prepared_dcommit(PreparedDcommitRequest {
        target: target.clone(),
        original_base_revision,
        original_base_oid,
        original_head,
        no_rebase: ctx.no_rebase,
        config_fingerprint: config_fingerprint.clone(),
        plans,
    })
    .map_err(|error| error.to_string())?;
    let journal_directory = if let Some(located) = active {
        if located.journal.target != target {
            return Err(
                "unfinished dcommit journal target does not match the resolved target".to_string(),
            );
        }
        let mut journal = located.journal;
        reconcile_recovery_config_fingerprint(
            &mut journal.config_fingerprint,
            &config_fingerprint,
            &[&legacy_config_fingerprint_v3, &legacy_config_fingerprint_v2],
        )?;
        prepared.journal = journal;
        located.directory
    } else {
        ctx.rev_map_path
            .parent()
            .ok_or_else(|| "rev_map path has no metadata directory".to_string())?
            .join("dcommit-journal")
    };

    let temp = TempCheckout::new()?;
    run_svn(
        Some(&temp.root),
        &ctx.svn_options,
        &[
            "checkout".to_string(),
            "--quiet".to_string(),
            crate::svn::target_without_peg_revision(&target_url),
            "wc".to_string(),
        ],
    )?;
    let sink = WorkingCopyCommitSink {
        wc: &temp.wc,
        options: &ctx.svn_options,
        git: ctx.git,
        rev_map_path: ctx.rev_map_path,
    };
    let post_submit = FileSvnPostSubmit {
        git: ctx.git,
        original_base_oid: prepared.journal.original_base_oid.clone(),
        plans: prepared.plans.clone(),
        fetch_config: ctx.post_commit_fetch_config,
        rebase_shared: ctx.post_commit_fetch_shared.clone(),
        fetch_shared: ctx.post_commit_fetch_shared,
        expected_footer_url: ctx.expected_footer_url,
        expected_footer_uuid: ctx.expected_footer_uuid,
    };
    let persistence = JournalStorePersistence::new(JournalStore::new(journal_directory))
        .map_err(|error| error.to_string())?;
    let mut coordinator = Coordinator::new(sink, post_submit, persistence);
    if let Some(revision) = ctx.adopt_revision {
        coordinator
            .adopt_in_flight(&mut prepared, revision)
            .map_err(|error| error.to_string())?;
    }
    coordinator
        .run(&mut prepared)
        .map_err(|error| error.to_string())?;

    let mut out = format!(
        "Committed {} local Git commit(s)\n",
        prepared.journal.entries.len()
    );
    for entry in &prepared.journal.entries {
        let revision = match entry.state {
            EntryState::Submitted { svn_revision }
            | EntryState::FetchedVerified { svn_revision, .. } => svn_revision,
            _ => return Err("completed dcommit journal entry has no SVN revision".to_string()),
        };
        let label = commits
            .iter()
            .find(|commit| commit.id == entry.git_oid)
            .map_or_else(
                || entry.git_oid.chars().take(12).collect::<String>(),
                |commit| format!("{} {}", commit.short_id, commit.subject),
            );
        out.push_str(&format!("Committed {label} as r{revision}\n"));
    }
    if ctx.no_rebase {
        out.push_str("Skipped rebase (--no-rebase).\n");
    } else {
        out.push_str("Rebased onto tracked SVN ref.\n");
    }
    Ok(out)
}

fn reconcile_recovery_config_fingerprint(
    stored: &mut String,
    current: &str,
    legacy: &[&str],
) -> Result<(), String> {
    if stored == current {
        return Ok(());
    }
    if legacy.contains(&stored.as_str()) {
        current.clone_into(stored);
        return Ok(());
    }
    Err("unfinished dcommit journal configuration does not match this invocation".to_string())
}

fn new_plan_chain(source_refname: &str, commits: &[GitCommitSummary]) -> Vec<(String, String)> {
    let mut base = source_refname.to_string();
    commits
        .iter()
        .map(|commit| {
            let pair = (base.clone(), commit.id.clone());
            base.clone_from(&commit.id);
            pair
        })
        .collect()
}

fn build_dcommit_plans(
    ctx: &DcommitPlanningContext<'_>,
    target_url: &str,
    original_base_revision: u64,
    chain: &[(String, String)],
    skip_noop: bool,
) -> Result<Vec<DcommitPlan>, String> {
    let planner = DcommitPlanBuilder::new();
    let mut plans = Vec::new();
    for (base_oid, git_commit) in chain {
        let offset =
            u64::try_from(plans.len()).map_err(|_| "dcommit plan count exceeds u64".to_string())?;
        let revision = original_base_revision
            .checked_add(offset)
            .ok_or_else(|| "dcommit base revision exceeds u64".to_string())?;
        let base_revision =
            u32::try_from(revision).map_err(|_| "dcommit base revision exceeds u32".to_string())?;
        let mut plan = planner.build(
            DcommitPlanRequest {
                target: DcommitTarget {
                    url: target_url.to_string(),
                    repository_root: ctx.repository_root.to_string(),
                    repository_uuid: ctx.repository_uuid.to_string(),
                    git_ref: ctx.mapping_ref.to_string(),
                },
                base_revision,
                git_commit: git_commit.clone(),
                message: dcommit_message(&ctx.git.commit_message(git_commit)?),
                author: Some(ctx.git.commit_author(git_commit)?),
                mergeinfo: ctx.mergeinfo.map(str::to_owned),
                changes: ctx.git.diff_raw(base_oid, git_commit)?,
            },
            |path| ctx.git.show_file(git_commit, path),
        )?;
        let base_attributes = git_attributes(ctx.git, base_oid)?;
        let current_attributes = git_attributes(ctx.git, git_commit)?;
        merge_attribute_properties(
            &mut plan,
            base_attributes.as_deref(),
            current_attributes.as_deref(),
        );
        if !skip_noop || plan.has_svn_changes() {
            plans.push(plan);
        }
    }
    Ok(plans)
}

fn finish_noop_dcommit(git: &GitCli, mapping_ref: &str, no_rebase: bool) -> Result<String, String> {
    let mut out = "No changes to dcommit.\n".to_string();
    if no_rebase {
        out.push_str("Skipped rebase (--no-rebase).\n");
    } else {
        git.reset_mixed(mapping_ref)?;
        out.push_str("Reset to tracked SVN ref.\n");
    }
    Ok(out)
}

struct WorkingCopyCommitSink<'a> {
    wc: &'a Path,
    options: &'a DcommitSvnOptions,
    git: &'a GitCli,
    rev_map_path: &'a Path,
}

impl CommitSink for WorkingCopyCommitSink<'_> {
    fn remote_head(&mut self, target: &DcommitTargetIdentity) -> Result<RemoteHead, String> {
        let revision = svn_last_changed_revision(&target.commit_url, self.options)?;
        let tracking_oid = RevMap::open_existing(self.rev_map_path, self.git.object_format()?)?
            .max_record(true)?
            .ok_or_else(|| "dcommit target rev_map is empty".to_string())?
            .object_id_hex;
        Ok(RemoteHead {
            revision,
            tracking_oid,
        })
    }

    fn submit(&mut self, plan: &DcommitPlan, expected_base_revision: u64) -> Result<u64, String> {
        let expected_base = u32::try_from(expected_base_revision)
            .map_err(|_| "dcommit expected base revision exceeds u32".to_string())?;
        if plan.base_revision != expected_base {
            return Err(format!(
                "prepared plan base r{} does not match coordinator base r{expected_base}",
                plan.base_revision
            ));
        }
        let mut editor =
            WorkingCopyPlanEditor::new(self.wc, self.options, &plan.message, expected_base);
        SvnCommitEditor::new(PropertyMapper)
            .apply_plan(&mut editor, plan)
            .map(u64::from)
    }
}

struct FileSvnPostSubmit<'a> {
    git: &'a GitCli,
    original_base_oid: String,
    plans: Vec<DcommitPlan>,
    fetch_config: crate::config::SvnRemoteConfig,
    fetch_shared: crate::cli::SharedFetchArgs,
    rebase_shared: crate::cli::SharedFetchArgs,
    expected_footer_url: String,
    expected_footer_uuid: String,
}

impl PostSubmit for FileSvnPostSubmit<'_> {
    fn fetch_and_verify(
        &mut self,
        target: &DcommitTargetIdentity,
        entry: &JournalEntry,
        svn_revision: u64,
    ) -> Result<String, String> {
        let revision = u32::try_from(svn_revision)
            .map_err(|_| "submitted SVN revision exceeds u32".to_string())?;
        let mut fetch_shared = self.fetch_shared.clone();
        fetch_shared.revision = Some(revision.to_string());
        fetch::run_for_tracking_identity(
            self.git.work_tree().to_path_buf(),
            self.fetch_config.clone(),
            &target.mapping_ref,
            &fetch_shared,
        )?;
        let expected_tree = projected_tree_for_entry(
            self.git,
            &self.original_base_oid,
            &self.plans,
            &entry.git_oid,
        )?;
        verify_imported_dcommit(
            self.git,
            target,
            revision,
            ImportedDcommitExpectation {
                footer_url: &self.expected_footer_url,
                footer_uuid: &self.expected_footer_uuid,
                tree: &expected_tree,
                plans: &self.plans,
                git_oid: &entry.git_oid,
            },
        )
        .map_err(|error| {
            format!("SVN r{revision} was submitted but post-fetch verification failed: {error}")
        })
    }

    fn rebase(&mut self, _journal: &DcommitJournal) -> Result<(), String> {
        rebase::run_in_work_tree(
            self.git.work_tree().to_path_buf(),
            crate::cli::RebaseArgs {
                dry_run: false,
                verbose: false,
                local: false,
                fetch_all: false,
                merge: false,
                rebase_merges: false,
                strategy: None,
                shared: self.rebase_shared.clone(),
            },
        )
        .map(|_| ())
    }
}

struct ImportedDcommitExpectation<'a> {
    footer_url: &'a str,
    footer_uuid: &'a str,
    tree: &'a std::collections::BTreeMap<String, crate::git::GitTreeFile>,
    plans: &'a [DcommitPlan],
    git_oid: &'a str,
}

fn verify_imported_dcommit(
    git: &GitCli,
    target: &DcommitTargetIdentity,
    revision: u32,
    expected: ImportedDcommitExpectation<'_>,
) -> Result<String, String> {
    let mapped_oid = RevMap::open_existing(&target.rev_map_path, git.object_format()?)?
        .get(revision)?
        .ok_or_else(|| format!("rev_map has no object for r{revision}"))?;
    let ref_oid = git.rev_parse(&target.mapping_ref)?;
    if mapped_oid != ref_oid.trim() {
        return Err(format!(
            "tracking ref {} points to {}, but rev_map r{revision} points to {mapped_oid}",
            target.mapping_ref,
            ref_oid.trim()
        ));
    }
    let message = git.commit_message(&mapped_oid)?;
    let footer = message
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| "imported commit has no git-svn-id footer".to_string())?;
    let identity = GitSvnId::parse(footer.trim_end_matches('\r'))?;
    if identity.url != expected.footer_url
        || identity.uuid != expected.footer_uuid
        || identity.revision != revision
    {
        return Err(format!(
            "imported git-svn-id does not match expected {}@{revision} {}",
            expected.footer_url, expected.footer_uuid
        ));
    }
    let mut imported_tree = tree_map(git.tree_files(&mapped_oid)?);
    for plan in expected.plans {
        canonicalize_tree_keywords(&mut imported_tree, plan);
        if plan.git_commit == expected.git_oid {
            break;
        }
    }
    if imported_tree != *expected.tree {
        let mismatch = expected
            .tree
            .iter()
            .find(|(path, expected)| imported_tree.get(*path) != Some(*expected))
            .map(|(path, expected)| {
                let actual = imported_tree.get(path);
                format!(
                    "{path} (expected mode {} and {} bytes, imported {})",
                    expected.mode,
                    expected.content.len(),
                    actual.map_or_else(
                        || "missing".to_string(),
                        |file| format!("mode {} and {} bytes", file.mode, file.content.len())
                    )
                )
            })
            .or_else(|| {
                imported_tree
                    .keys()
                    .find(|path| !expected.tree.contains_key(*path))
                    .map(|path| format!("unexpected path {path}"))
            })
            .unwrap_or_else(|| "<unknown>".to_string());
        return Err(format!(
            "imported tree does not match the dcommit plan projection at {mismatch}"
        ));
    }
    Ok(mapped_oid)
}

fn projected_tree_for_entry(
    git: &GitCli,
    original_base_oid: &str,
    plans: &[DcommitPlan],
    git_oid: &str,
) -> Result<std::collections::BTreeMap<String, crate::git::GitTreeFile>, String> {
    let mut tree = tree_map(git.tree_files(original_base_oid)?);
    for plan in plans {
        apply_plan_to_tree(&mut tree, plan);
        if plan.git_commit == git_oid {
            return Ok(tree);
        }
    }
    Err(format!(
        "dcommit plan queue has no projection for Git commit {git_oid}"
    ))
}

fn svn_last_changed_revision(url: &str, options: &DcommitSvnOptions) -> Result<u64, String> {
    svn_info_item(url, "last-changed-revision", options)?
        .parse::<u64>()
        .map_err(|error| format!("invalid SVN remote revision: {error}"))
}

fn validate_new_dcommit_base(
    git: &GitCli,
    mapping_ref: &str,
    rev_map_path: &Path,
    target_url: &str,
    options: &DcommitSvnOptions,
) -> Result<(u64, String), String> {
    let (expected_revision, mapping_oid) = validate_tracking_base(git, mapping_ref, rev_map_path)?;
    let actual_revision = svn_last_changed_revision(target_url, options)?;
    if actual_revision > expected_revision {
        return Err(format!(
            "SVN remote advanced from expected r{expected_revision} to r{actual_revision}; refusing to submit"
        ));
    }
    if actual_revision != expected_revision {
        return Err(format!(
            "SVN remote revision mismatch: expected r{expected_revision}, found r{actual_revision}; refusing to submit"
        ));
    }
    Ok((expected_revision, mapping_oid))
}

fn validate_tracking_base(
    git: &GitCli,
    mapping_ref: &str,
    rev_map_path: &Path,
) -> Result<(u64, String), String> {
    let record = RevMap::open_existing(rev_map_path, git.object_format()?)?
        .max_record(true)?
        .ok_or_else(|| "dcommit target rev_map is empty".to_string())?;
    let mapping_oid = git.rev_parse(mapping_ref)?.trim().to_string();
    if mapping_oid != record.object_id_hex {
        return Err(format!(
            "dcommit tracking ref {mapping_ref} does not match its rev_map; expected {}, found {mapping_oid}",
            record.object_id_hex
        ));
    }

    let expected_revision = u64::from(record.revision);
    Ok((expected_revision, mapping_oid))
}

fn validate_svn_repository_uuid(
    url: &str,
    expected_uuid: &str,
    options: &DcommitSvnOptions,
) -> Result<(), String> {
    let actual_uuid = svn_info_item(url, "repos-uuid", options)?;
    if actual_uuid != expected_uuid {
        return Err(format!(
            "dcommit target repository UUID mismatch: expected {expected_uuid}, found {actual_uuid} at {url}; refusing to write"
        ));
    }
    Ok(())
}

fn svn_info_item(url: &str, item: &str, options: &DcommitSvnOptions) -> Result<String, String> {
    Ok(run_svn_output(
        None,
        options,
        &[
            "info".to_string(),
            "--show-item".to_string(),
            item.to_string(),
            crate::svn::target_without_peg_revision(url),
        ],
    )?
    .trim()
    .to_string())
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
        let mut command_args = vec!["--non-interactive".to_string()];
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
