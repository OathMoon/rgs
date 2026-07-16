use crate::cli::ResetArgs;
use crate::commands::reset_transaction;
use crate::commands::resolver::resolve_tracked_svn;
use crate::dcommit::journal_registry::{RepositoryDcommitLock, discover_repository_journals};
use crate::git::GitCli;

pub fn run(args: ResetArgs) -> Result<(), String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: ResetArgs,
) -> Result<(), String> {
    let work_tree = work_tree.into();
    let git = GitCli::new(&work_tree);
    let svn_metadata_root = git.work_tree().join(git.git_dir()?).join("svn");
    let _lock =
        RepositoryDcommitLock::acquire(&svn_metadata_root).map_err(|error| error.to_string())?;
    if discover_repository_journals(&svn_metadata_root)
        .map_err(|error| error.to_string())?
        .and_then(|discovery| discovery.active)
        .is_some()
    {
        return Err("reset is blocked by an unfinished dcommit journal".to_string());
    }
    reset_transaction::recover_pending(&git)?;
    let tracked = resolve_tracked_svn(work_tree)?;
    let revision = parse_revision(&args.revision)?;
    let target_revision = if args.parent {
        revision
            .checked_sub(1)
            .ok_or_else(|| "cannot reset to parent of revision 0".to_string())?
    } else {
        revision
    };

    let Some(record) = tracked
        .records()?
        .into_iter()
        .find(|record| record.revision == target_revision)
    else {
        return Err(format!(
            "no Git commit found for SVN revision r{target_revision}"
        ));
    };
    if record.object_id_hex.chars().all(|c| c == '0') {
        return Err(format!(
            "no Git commit found for SVN revision r{target_revision}"
        ));
    }

    reset_transaction::execute(
        &tracked.git,
        &tracked.refname,
        &tracked.rev_map_path,
        target_revision,
        &record.object_id_hex,
    )
}

fn parse_revision(value: &str) -> Result<u32, String> {
    value
        .strip_prefix('r')
        .unwrap_or(value)
        .parse::<u32>()
        .map_err(|_| format!("invalid SVN revision: {value}"))
}
