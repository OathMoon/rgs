use crate::cli::FindRevArgs;
use crate::commands::resolver::resolve_tracked_svn;

pub fn run(args: FindRevArgs) -> Result<String, String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: FindRevArgs,
) -> Result<String, String> {
    let tracked = resolve_tracked_svn(work_tree)?;
    if let Some(revision) = parse_revision(&args.rev_or_commit) {
        let records = tracked.records()?;
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
        let commit = tracked.git.rev_parse(&args.rev_or_commit)?;
        let commit = commit.trim();
        let revision = tracked
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
