use crate::cli::InfoArgs;
use crate::commands::resolver::resolve_tracked_svn;

pub fn run(args: InfoArgs) -> Result<String, String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: InfoArgs,
) -> Result<String, String> {
    let tracked = resolve_tracked_svn(work_tree)?;
    if args.url {
        return Ok(format!("{}\n", tracked.config.url));
    }
    let revision = tracked
        .max_record()?
        .map(|record| record.revision.to_string())
        .unwrap_or_else(|| "0".to_string());

    Ok(format!(
        "URL: {}\nRepository Root: {}\nRepository UUID: {}\nRevision: {}\n",
        tracked.config.url, tracked.config.url, tracked.uuid, revision
    ))
}
