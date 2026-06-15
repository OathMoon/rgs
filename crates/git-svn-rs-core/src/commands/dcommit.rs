use crate::cli::DcommitArgs;
use crate::commands::resolver::resolve_tracked_svn;

pub fn run(args: DcommitArgs) -> Result<String, String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: DcommitArgs,
) -> Result<String, String> {
    if args.mergeinfo.is_some() {
        return Err(
            "--mergeinfo is parsed for compatibility, but mergeinfo write-back is not implemented in v1"
                .to_string(),
        );
    }

    if !args.dry_run {
        return Err(
            "dcommit without --dry-run is not implemented; production SVN write-back is not available yet"
                .to_string(),
        );
    }

    let tracked = resolve_tracked_svn(work_tree)?;
    let revision = tracked
        .max_record()?
        .map(|record| record.revision)
        .unwrap_or(0);
    let target_url = args.commit_url.as_deref().unwrap_or(&tracked.config.url);
    let commits = if tracked.git.rev_parse("HEAD").is_ok() {
        tracked
            .git
            .commit_summaries_between(&tracked.refname, "HEAD")?
    } else {
        Vec::new()
    };

    let mut out = format!(
        "Dcommit dry-run against {target_url} ({}, r{revision})\n",
        tracked.refname
    );
    if commits.is_empty() {
        out.push_str("No local commits to dcommit.\n");
        return Ok(out);
    }

    out.push_str(&format!(
        "Would commit {} local Git commit(s):\n",
        commits.len()
    ));
    for commit in commits {
        out.push_str(&format!("{} {}\n", commit.short_id, commit.subject));
    }
    Ok(out)
}
