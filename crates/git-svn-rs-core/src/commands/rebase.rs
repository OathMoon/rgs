use crate::cli::RebaseArgs;
use crate::commands::{fetch, resolver::resolve_tracked_svn};

pub fn run(args: RebaseArgs) -> Result<String, String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: RebaseArgs,
) -> Result<String, String> {
    let work_tree = work_tree.into();
    let tracked = resolve_tracked_svn(work_tree.clone())?;
    if tracked.config.no_metadata {
        return Err("fetch is unavailable after a --no-metadata one-shot import".to_string());
    }
    if args.dry_run {
        return Ok(String::new());
    }
    if !tracked.git.is_work_tree_clean()? {
        return Err("rebase requires a clean index and work tree".to_string());
    }

    if !args.local {
        fetch::run_for_tracking_identity(
            work_tree.clone(),
            tracked.config,
            &tracked.refname,
            &args.shared,
        )?;
    }
    let tracked = resolve_tracked_svn(work_tree)?;
    tracked
        .git
        .rebase(&tracked.refname, args.merge, args.strategy.as_deref())
}
