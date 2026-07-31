use crate::cli::FindRevArgs;
use crate::commands::resolver::{resolve_tracked_svn, resolve_tracked_svn_at};
use crate::rev_map::RevMapRecord;

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
        if tracked.config.no_metadata {
            return Err(
                "find-rev is unavailable for --no-metadata imports when resolving SVN revisions"
                    .to_string(),
            );
        }
        let records = tracked.open_rev_map()?.records()?;
        let record = select_revision_record(&records, revision, args.before, args.after);
        let Some(record) = record else {
            return Ok(String::new());
        };
        Ok(format!("{}\n", record.object_id_hex))
    } else {
        if args.treeish.is_some() {
            return Err("find-rev accepts a tree-ish scope only with an SVN revision".to_string());
        }
        let tracked = resolve_tracked_svn_at(&work_tree, &args.rev_or_commit)
            .or_else(|_| resolve_tracked_svn(&work_tree))?;
        let commit = tracked.git.rev_parse(&args.rev_or_commit)?;
        if tracked.config.no_metadata {
            return Ok(String::new());
        }
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

fn select_revision_record(
    records: &[RevMapRecord],
    revision: u32,
    before: bool,
    after: bool,
) -> Option<&RevMapRecord> {
    let mut records = records.iter().filter(|record| !record.has_zero_object_id());
    if before {
        records.rfind(|record| record.revision <= revision)
    } else if after {
        records.find(|record| record.revision >= revision)
    } else {
        records.find(|record| record.revision == revision)
    }
}

#[cfg(test)]
mod tests {
    use super::select_revision_record;
    use crate::rev_map::RevMapRecord;

    fn record(revision: u32, object_id: char) -> RevMapRecord {
        RevMapRecord {
            revision,
            object_id_hex: object_id.to_string().repeat(40),
        }
    }

    #[test]
    fn trailing_zero_does_not_hide_the_nearest_before_record() {
        let records = vec![record(1, 'a'), record(2, 'b'), record(3, '0')];

        assert_eq!(
            select_revision_record(&records, 3, true, false).map(|record| record.revision),
            Some(2)
        );
        assert_eq!(
            select_revision_record(&records, 4, true, false).map(|record| record.revision),
            Some(2)
        );
        assert_eq!(select_revision_record(&records, 3, false, false), None);
        assert_eq!(select_revision_record(&records, 3, false, true), None);
    }
}
