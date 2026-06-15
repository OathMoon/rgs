use crate::cli::ResetArgs;
use crate::commands::resolver::resolve_tracked_svn;

pub fn run(args: ResetArgs) -> Result<(), String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: ResetArgs,
) -> Result<(), String> {
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

    tracked
        .git
        .update_ref(&tracked.refname, &record.object_id_hex)?;
    tracked
        .open_rev_map()?
        .reset_to(target_revision, &record.object_id_hex)
}

fn parse_revision(value: &str) -> Result<u32, String> {
    value
        .strip_prefix('r')
        .unwrap_or(value)
        .parse::<u32>()
        .map_err(|_| format!("invalid SVN revision: {value}"))
}
