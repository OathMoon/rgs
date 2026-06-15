use crate::cli::{FetchArgs, RebaseArgs};
use crate::commands::{fetch, resolver::resolve_tracked_svn};

pub fn run(args: RebaseArgs) -> Result<String, String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: RebaseArgs,
) -> Result<String, String> {
    let work_tree = work_tree.into();
    let refname =
        configured_refname(&work_tree).unwrap_or_else(|| "refs/remotes/git-svn".to_string());
    if args.dry_run {
        return Ok(format!("would run fetch\nwould run git rebase {refname}\n"));
    }

    fetch::run_in_work_tree(
        work_tree.clone(),
        FetchArgs {
            remote: None,
            shared: args.shared,
            fetch_all: false,
            parent: false,
        },
    )?;
    let tracked = resolve_tracked_svn(work_tree)?;
    tracked
        .git
        .rebase(&tracked.refname, args.merge, args.strategy.as_deref())
}

fn configured_refname(work_tree: &std::path::Path) -> Option<String> {
    let git = crate::git::GitCli::new(work_tree);
    git.config_get_all("svn-remote.svn.fetch")
        .ok()?
        .into_iter()
        .find_map(|mapping| {
            mapping
                .split_once(':')
                .map(|(_, refname)| refname.to_string())
        })
}
