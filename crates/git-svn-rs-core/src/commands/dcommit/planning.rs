use super::*;

pub(super) struct DcommitPlanningContext<'a> {
    pub(super) git: &'a GitCli,
    pub(super) repository_root: &'a str,
    pub(super) repository_uuid: &'a str,
    pub(super) mapping_ref: &'a str,
    pub(super) mergeinfo: Option<&'a str>,
}

pub(super) fn dcommit_dry_run(
    tracked: &crate::commands::resolver::TrackedSvn,
    args: &DcommitArgs,
    target: &ResolvedDcommitTarget,
    commits: &[GitCommitSummary],
) -> Result<String, String> {
    let (base_revision, _) = if is_svn_cli_write_back_url(&target.commit_url)
        && is_svn_cli_write_back_url(&tracked.config.url)
    {
        let svn_options = resolved_dcommit_svn_options(tracked, args, &target.commit_url)?;
        validate_svn_repository_uuid(&target.commit_url, &tracked.uuid, &svn_options)?;
        validate_new_dcommit_base(
            &tracked.git,
            &target.mapping_ref,
            &target.rev_map_path,
            &target.commit_url,
            &svn_options,
        )?
    } else {
        validate_tracking_base(&tracked.git, &target.mapping_ref, &target.rev_map_path)?
    };
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

pub(super) fn dcommit_message(message: &str) -> String {
    message.trim_end_matches(['\r', '\n']).to_string()
}

pub(super) fn reconcile_recovery_config_fingerprint(
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

pub(super) fn new_plan_chain(
    source_refname: &str,
    commits: &[GitCommitSummary],
) -> Vec<(String, String)> {
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

pub(super) fn build_dcommit_plans(
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

pub(super) fn finish_noop_dcommit(
    git: &GitCli,
    mapping_ref: &str,
    no_rebase: bool,
) -> Result<String, String> {
    let mut out = "No changes to dcommit.\n".to_string();
    if no_rebase {
        out.push_str("Skipped rebase (--no-rebase).\n");
    } else {
        git.reset_mixed(mapping_ref)?;
        out.push_str("Reset to tracked SVN ref.\n");
    }
    Ok(out)
}

pub(super) fn git_attributes(git: &GitCli, commit: &str) -> Result<Option<String>, String> {
    match git.show_file(commit, ".gitattributes") {
        Ok(content) => String::from_utf8(content)
            .map(Some)
            .map_err(|error| format!(".gitattributes is not valid UTF-8: {error}")),
        Err(_) => Ok(None),
    }
}
