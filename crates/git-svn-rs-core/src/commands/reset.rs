use crate::cli::ResetArgs;
use crate::commands::reset_transaction;
use crate::commands::resolver::resolve_tracked_svn;
use crate::config::{read_svn_remote_config, svn_remote_names};
use crate::dcommit::journal_registry::{RepositoryDcommitLock, discover_repository_journals};
use crate::git::GitCli;

pub fn run(args: ResetArgs) -> Result<String, String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: ResetArgs,
) -> Result<String, String> {
    let revision = requested_revision(&args)?;
    let work_tree = work_tree.into();
    let git = GitCli::new(&work_tree);
    reject_configured_svm_reset(&git)?;
    let svn_metadata_root = git.work_tree().join(git.git_dir()?).join("svn");
    let _lock =
        RepositoryDcommitLock::acquire(&svn_metadata_root).map_err(|error| error.to_string())?;
    if discover_repository_journals(&svn_metadata_root)
        .map_err(|error| error.to_string())?
        .and_then(|discovery| discovery.active)
        .is_some()
    {
        return Err("reset is blocked by an unfinished dcommit journal".to_string());
    }
    reset_transaction::recover_pending(&git)?;
    let tracked = resolve_tracked_svn(work_tree)?;
    if tracked.config.no_metadata {
        return Err(
            "reset is unavailable for --no-metadata imports because history has no git-svn-id metadata"
                .to_string(),
        );
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
    let record = if args.parent {
        records
            .into_iter()
            .filter(|record| record.revision < revision && !record.has_zero_object_id())
            .max_by_key(|record| record.revision)
            .ok_or_else(|| format!("no Git commit found before SVN revision r{revision}"))?
    } else {
        records
            .into_iter()
            .find(|record| record.revision == revision && !record.has_zero_object_id())
            .ok_or_else(|| format!("no Git commit found for SVN revision r{revision}"))?
    };
    let target_revision = record.revision;

    reset_transaction::execute(
        &tracked.git,
        &tracked.refname,
        &tracked.rev_map_path,
        target_revision,
        &record.object_id_hex,
    )?;
    Ok(format!(
        "r{target_revision} = {} ({})\n",
        record.object_id_hex, tracked.refname
    ))
}

fn reject_configured_svm_reset(git: &GitCli) -> Result<(), String> {
    for remote in svn_remote_names(git)? {
        if read_svn_remote_config(git, &remote)?.use_svm_props {
            return Err(
                "reset is unavailable for useSvmProps mirrors until both revision maps can be reset atomically"
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn requested_revision(args: &ResetArgs) -> Result<u32, String> {
    args.revision
        .as_deref()
        .or(args.revision_option.as_deref())
        .ok_or_else(|| "SVN revision required".to_string())
        .and_then(parse_revision)
}

fn parse_revision(value: &str) -> Result<u32, String> {
    value
        .strip_prefix('r')
        .unwrap_or(value)
        .parse::<u32>()
        .map_err(|_| format!("invalid SVN revision: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svm_reset_rejects_before_lock_or_pending_reset_recovery() {
        let temp = tempfile::tempdir().unwrap();
        let git = GitCli::new(temp.path());
        git.init().unwrap();
        git.config_set("svn-remote.svn.url", "mock://repo").unwrap();
        git.config_set("svn-remote.svn.fetch", "trunk:refs/remotes/git-svn")
            .unwrap();
        git.config_set("svn-remote.svn.useSvmProps", "true")
            .unwrap();
        let metadata_root = temp.path().join(".git/svn");
        std::fs::create_dir_all(&metadata_root).unwrap();
        let pending = metadata_root.join("reset-journal");
        std::fs::write(&pending, b"pending reset must remain untouched").unwrap();
        let _lock = RepositoryDcommitLock::acquire(&metadata_root).unwrap();

        let error = run_in_work_tree(
            temp.path(),
            ResetArgs {
                revision: Some("1".to_string()),
                revision_option: None,
                parent: false,
            },
        )
        .unwrap_err();

        assert!(error.contains("useSvmProps"), "{error}");
        assert_eq!(
            std::fs::read(pending).unwrap(),
            b"pending reset must remain untouched"
        );
    }
}
