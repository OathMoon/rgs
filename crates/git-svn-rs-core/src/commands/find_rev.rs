use crate::cli::FindRevArgs;
use crate::commands::resolver::{resolve_tracked_svn, resolve_tracked_svn_at};

pub fn run(args: FindRevArgs) -> Result<String, String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: FindRevArgs,
) -> Result<String, String> {
    let work_tree = work_tree.into();
    if let Some(revision) = parse_revision(&args.rev_or_commit) {
        let tracked = match args.treeish.as_deref() {
            Some(treeish) => resolve_tracked_svn_at(&work_tree, treeish)?,
            None => resolve_tracked_svn(&work_tree)?,
        };
        let records = tracked.open_rev_map()?.records()?;
        let record = if args.before {
            records.into_iter().rfind(|r| r.revision <= revision)
        } else if args.after {
            records.into_iter().find(|r| r.revision >= revision)
        } else {
            records.into_iter().find(|r| r.revision == revision)
        };
        let Some(record) = record else {
            return Ok(String::new());
        };
        if record.object_id_hex.chars().all(|c| c == '0') {
            Ok(String::new())
        } else {
            Ok(format!("{}\n", record.object_id_hex))
        }
    } else {
        if args.treeish.is_some() {
            return Err("find-rev accepts a tree-ish scope only with an SVN revision".to_string());
        }
        let tracked = resolve_tracked_svn_at(&work_tree, &args.rev_or_commit)
            .or_else(|_| resolve_tracked_svn(&work_tree))?;
        let commit = tracked.git.rev_parse(&args.rev_or_commit)?;
        let commit = commit.trim();
        let revision = tracked
            .open_rev_map()?
            .records()?
            .into_iter()
            .find(|record| record.object_id_hex == commit)
            .map(|record| record.revision);
        Ok(revision.map(|rev| format!("{rev}\n")).unwrap_or_default())
    }
}

fn parse_revision(value: &str) -> Option<u32> {
    value.strip_prefix('r').unwrap_or(value).parse::<u32>().ok()
}
