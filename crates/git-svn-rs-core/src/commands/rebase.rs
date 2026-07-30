use crate::cli::RebaseArgs;
use crate::commands::{fetch, resolver::resolve_tracked_svn};
use crate::path_url::add_path_to_url;

pub fn run(args: RebaseArgs) -> Result<String, String> {
    run_in_work_tree(".", args)
}

pub fn run_with_inherited_stderr(args: RebaseArgs) -> Result<String, String> {
    run_in_work_tree_mode(".", args, true)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: RebaseArgs,
) -> Result<String, String> {
    run_in_work_tree_mode(work_tree, args, false)
}

fn run_in_work_tree_mode(
    work_tree: impl Into<std::path::PathBuf>,
    args: RebaseArgs,
    inherit_rebase_stderr: bool,
) -> Result<String, String> {
    let work_tree = work_tree.into();
    let tracked = resolve_tracked_svn(work_tree.clone())?;
    if tracked.config.no_metadata {
        return Err("fetch is unavailable after a --no-metadata one-shot import".to_string());
    }
    crate::tracking_state::validate_existing_tracking_state(
        &tracked.git,
        &tracked.config,
        &tracked.refname,
        &tracked.svn_path,
        &tracked.uuid,
        &tracked.rev_map_path,
    )?;
    if args.dry_run {
        let root = tracked
            .config
            .rewrite_root
            .as_ref()
            .unwrap_or(&tracked.config.url);
        let url = add_path_to_url(root, &tracked.svn_path);
        return Ok(format!(
            "Remote Branch: {}\nSVN URL: {url}\n",
            tracked.refname
        ));
    }
    if !tracked.git.is_index_and_tracked_work_tree_clean()? {
        return Err("rebase requires a clean index and work tree".to_string());
    }

    if !args.local {
        if args.fetch_all {
            fetch::run_for_tracking_remote(work_tree, tracked.config.clone(), &args.shared)?;
        } else {
            fetch::run_for_tracking_identity(
                work_tree,
                tracked.config.clone(),
                &tracked.refname,
                &args.shared,
            )?;
        }
    }
    if inherit_rebase_stderr {
        tracked.git.rebase_with_inherited_stderr(
            &tracked.refname,
            args.verbose,
            args.merge,
            args.strategy.as_deref(),
            args.rebase_merges,
        )
    } else {
        tracked.git.rebase(
            &tracked.refname,
            args.verbose,
            args.merge,
            args.strategy.as_deref(),
            args.rebase_merges,
        )
    }
}
