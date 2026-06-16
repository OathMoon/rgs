use crate::cli::FindRevArgs;
use crate::commands::resolver::resolve_tracked_svn;
use crate::rev_map::{ObjectFormat, RevMap, RevMapRecord};
use std::path::{Path, PathBuf};

pub fn run(args: FindRevArgs) -> Result<String, String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: FindRevArgs,
) -> Result<String, String> {
    let tracked = resolve_tracked_svn(work_tree)?;
    let git_dir = tracked.git.git_dir()?;
    if let Some(revision) = parse_revision(&args.rev_or_commit) {
        let records = all_rev_map_records(&tracked.git.work_tree().join(&git_dir))?;
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
        let revision = all_rev_map_records(&tracked.git.work_tree().join(git_dir))?
            .into_iter()
            .find(|record| record.object_id_hex == commit)
            .map(|record| record.revision);
        Ok(revision.map(|rev| format!("{rev}\n")).unwrap_or_default())
    }
}

fn all_rev_map_records(git_dir: &Path) -> Result<Vec<RevMapRecord>, String> {
    let mut paths = Vec::new();
    collect_rev_map_paths(&git_dir.join("svn"), &mut paths)?;
    paths.sort();

    let mut records = Vec::new();
    for path in paths {
        records.extend(RevMap::open(path, ObjectFormat::Sha1)?.records()?);
    }
    records.sort_by_key(|record| record.revision);
    Ok(records)
}

fn collect_rev_map_paths(path: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_rev_map_paths(&path, paths)?;
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".rev_map.") && !name.ends_with(".lock"))
        {
            paths.push(path);
        }
    }
    Ok(())
}

fn parse_revision(value: &str) -> Option<u32> {
    value.strip_prefix('r').unwrap_or(value).parse::<u32>().ok()
}
