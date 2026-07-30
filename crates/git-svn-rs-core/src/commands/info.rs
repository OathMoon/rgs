use crate::cli::InfoArgs;
use crate::commands::resolver::resolve_tracked_svn;
use crate::rev_map::RevMapRecord;

pub fn run(args: InfoArgs) -> Result<String, String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: InfoArgs,
) -> Result<String, String> {
    let tracked = resolve_tracked_svn(work_tree)?;
    let url = tracked.config.metadata_url(&tracked.svn_path);
    if args.url {
        return Ok(format!("{url}\n"));
    }
    crate::tracking_state::validate_existing_tracking_state(
        &tracked.git,
        &tracked.config,
        &tracked.refname,
        &tracked.svn_path,
        &tracked.uuid,
        &tracked.rev_map_path,
    )?;
    let records = tracked.records()?;
    let first_parent_history = tracked.git.first_parent_history("HEAD")?;
    let revision = revision_for_first_parent(&records, &first_parent_history)
        .ok_or_else(|| {
            format!(
                "unable to determine an SVN revision from HEAD history for {}",
                tracked.refname
            )
        })?
        .to_string();
    let repository_root = tracked
        .git
        .git_svn_metadata_get(&format!("svn-remote.{}.reposRoot", tracked.config.name))?
        .unwrap_or_else(|| tracked.config.url.clone());

    Ok(format!(
        "URL: {}\nRepository Root: {}\nRepository UUID: {}\nRevision: {}\n",
        url, repository_root, tracked.uuid, revision
    ))
}

fn revision_for_first_parent(
    records: &[RevMapRecord],
    first_parent_history: &[String],
) -> Option<u32> {
    first_parent_history.iter().find_map(|commit| {
        records
            .iter()
            .find(|record| record.object_id_hex == *commit)
            .map(|record| record.revision)
    })
}

#[cfg(test)]
mod tests {
    use super::revision_for_first_parent;
    use crate::rev_map::RevMapRecord;

    #[test]
    fn revision_uses_the_nearest_rev_map_record_in_first_parent_history() {
        let records = vec![
            RevMapRecord {
                revision: 1,
                object_id_hex: "first".to_string(),
            },
            RevMapRecord {
                revision: 3,
                object_id_hex: "latest".to_string(),
            },
        ];

        assert_eq!(
            revision_for_first_parent(&records, &["local".to_string(), "first".to_string()]),
            Some(1)
        );
    }
}
