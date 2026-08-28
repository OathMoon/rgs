use super::*;

pub(super) fn reject_read_mirror_dcommit(
    config: &crate::config::SvnRemoteConfig,
    before_import_recovery: bool,
) -> Result<(), String> {
    let mode = if config.use_svm_props {
        Some("useSvmProps")
    } else if config.use_svnsync_props {
        Some("useSvnsyncProps")
    } else {
        None
    };
    let Some(mode) = mode else {
        return Ok(());
    };
    if before_import_recovery {
        Err(format!(
            "dcommit is unavailable for {mode} mirrors; refusing to recover an import batch before rejecting mirror write-back"
        ))
    } else {
        Err(format!(
            "dcommit is unavailable for {mode} mirrors; refusing to write through a read mirror"
        ))
    }
}

pub(super) fn reject_completed_ledger_overlap(
    discovery: Option<&JournalDiscovery>,
    commits: &[GitCommitSummary],
) -> Result<(), String> {
    if let Some(completed) = discovery.and_then(|discovery| {
        discovery.completed.iter().find(|located| {
            located.journal.entries.iter().any(|entry| {
                commits
                    .iter()
                    .any(|commit| commit.id.as_str() == entry.git_oid)
            })
        })
    }) {
        return Err(format!(
            "local commits overlap completed dcommit ledger at {}; rebase or reset before dcommit",
            completed.directory.display()
        ));
    }
    Ok(())
}

pub(super) fn svn_last_changed_revision(
    url: &str,
    options: &DcommitSvnOptions,
) -> Result<u64, String> {
    svn_info_item(url, "last-changed-revision", options)?
        .parse::<u64>()
        .map_err(|error| format!("invalid SVN remote revision: {error}"))
}

pub(super) fn validate_new_dcommit_base(
    git: &GitCli,
    mapping_ref: &str,
    rev_map_path: &Path,
    target_url: &str,
    options: &DcommitSvnOptions,
) -> Result<(u64, String), String> {
    let (expected_revision, mapping_oid) = validate_tracking_base(git, mapping_ref, rev_map_path)?;
    let actual_revision = svn_last_changed_revision(target_url, options)?;
    if actual_revision > expected_revision {
        return Err(format!(
            "SVN remote advanced from expected r{expected_revision} to r{actual_revision}; refusing to submit"
        ));
    }
    if actual_revision != expected_revision {
        return Err(format!(
            "SVN remote revision mismatch: expected r{expected_revision}, found r{actual_revision}; refusing to submit"
        ));
    }
    Ok((expected_revision, mapping_oid))
}

pub(super) fn validate_tracking_base(
    git: &GitCli,
    mapping_ref: &str,
    rev_map_path: &Path,
) -> Result<(u64, String), String> {
    let record = RevMap::open_existing(rev_map_path, git.object_format()?)?
        .max_record(true)?
        .ok_or_else(|| "dcommit target rev_map is empty".to_string())?;
    let mapping_oid = git.rev_parse(mapping_ref)?.trim().to_string();
    if mapping_oid != record.object_id_hex {
        return Err(format!(
            "dcommit tracking ref {mapping_ref} does not match its rev_map; expected {}, found {mapping_oid}",
            record.object_id_hex
        ));
    }

    let expected_revision = u64::from(record.revision);
    Ok((expected_revision, mapping_oid))
}

pub(super) fn validate_svn_repository_uuid(
    url: &str,
    expected_uuid: &str,
    options: &DcommitSvnOptions,
) -> Result<(), String> {
    let actual_uuid = svn_info_item(url, "repos-uuid", options)?;
    if actual_uuid != expected_uuid {
        return Err(format!(
            "dcommit target repository UUID mismatch: expected {expected_uuid}, found {actual_uuid} at {url}; refusing to write"
        ));
    }
    Ok(())
}

pub(super) fn svn_info_item(
    url: &str,
    item: &str,
    options: &DcommitSvnOptions,
) -> Result<String, String> {
    Ok(run_svn_output(
        None,
        options,
        &[
            "info".to_string(),
            "--show-item".to_string(),
            item.to_string(),
            crate::svn::target_without_peg_revision(url),
        ],
    )?
    .trim()
    .to_string())
}
