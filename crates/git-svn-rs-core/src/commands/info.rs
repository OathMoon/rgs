use crate::cli::InfoArgs;
use crate::commands::resolver::resolve_tracked_svn;
use crate::path_url::add_path_to_url;

pub fn run(args: InfoArgs) -> Result<String, String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: InfoArgs,
) -> Result<String, String> {
    let tracked = resolve_tracked_svn(work_tree)?;
    let url = add_path_to_url(&tracked.config.url, &tracked.svn_path);
    if args.url {
        return Ok(format!("{url}\n"));
    }
    let revision = tracked
        .max_record()?
        .map(|record| record.revision.to_string())
        .unwrap_or_else(|| "0".to_string());
    let repository_root = tracked
        .git
        .git_svn_metadata_get(&format!("svn-remote.{}.reposRoot", tracked.config.name))?
        .unwrap_or_else(|| tracked.config.url.clone());

    Ok(format!(
        "URL: {}\nRepository Root: {}\nRepository UUID: {}\nRevision: {}\n",
        url, repository_root, tracked.uuid, revision
    ))
}
