use super::*;

pub(super) struct FileSvnPostSubmit<'a> {
    pub(super) git: &'a GitCli,
    pub(super) original_base_oid: String,
    pub(super) plans: Vec<DcommitPlan>,
    pub(super) fetch_config: crate::config::SvnRemoteConfig,
    pub(super) fetch_shared: crate::cli::SharedFetchArgs,
    pub(super) rebase_shared: crate::cli::SharedFetchArgs,
    pub(super) expected_footer_url: String,
    pub(super) expected_footer_uuid: String,
}

impl PostSubmit for FileSvnPostSubmit<'_> {
    fn fetch_and_verify(
        &mut self,
        target: &DcommitTargetIdentity,
        entry: &JournalEntry,
        svn_revision: u64,
    ) -> Result<String, String> {
        let revision = u32::try_from(svn_revision)
            .map_err(|_| "submitted SVN revision exceeds u32".to_string())?;
        let mut fetch_shared = self.fetch_shared.clone();
        fetch_shared.revision = Some(revision.to_string());
        fetch::run_for_tracking_identity(
            self.git.work_tree().to_path_buf(),
            self.fetch_config.clone(),
            &target.mapping_ref,
            &fetch_shared,
        )?;
        let expected_tree = projected_tree_for_entry(
            self.git,
            &self.original_base_oid,
            &self.plans,
            &entry.git_oid,
        )?;
        verify_imported_dcommit(
            self.git,
            target,
            revision,
            ImportedDcommitExpectation {
                footer_url: &self.expected_footer_url,
                footer_uuid: &self.expected_footer_uuid,
                tree: &expected_tree,
                plans: &self.plans,
                git_oid: &entry.git_oid,
            },
        )
        .map_err(|error| {
            format!("SVN r{revision} was submitted but post-fetch verification failed: {error}")
        })
    }

    fn rebase(&mut self, _journal: &DcommitJournal) -> Result<(), String> {
        rebase::run_in_work_tree(
            self.git.work_tree().to_path_buf(),
            crate::cli::RebaseArgs {
                dry_run: false,
                verbose: false,
                local: false,
                fetch_all: false,
                merge: false,
                rebase_merges: false,
                strategy: None,
                shared: self.rebase_shared.clone(),
            },
        )
        .map(|_| ())
    }
}

pub(super) struct ImportedDcommitExpectation<'a> {
    footer_url: &'a str,
    footer_uuid: &'a str,
    tree: &'a std::collections::BTreeMap<String, crate::git::GitTreeFile>,
    plans: &'a [DcommitPlan],
    git_oid: &'a str,
}

pub(super) fn verify_imported_dcommit(
    git: &GitCli,
    target: &DcommitTargetIdentity,
    revision: u32,
    expected: ImportedDcommitExpectation<'_>,
) -> Result<String, String> {
    let mapped_oid = RevMap::open_existing(&target.rev_map_path, git.object_format()?)?
        .get(revision)?
        .ok_or_else(|| format!("rev_map has no object for r{revision}"))?;
    let ref_oid = git.rev_parse(&target.mapping_ref)?;
    if mapped_oid != ref_oid.trim() {
        return Err(format!(
            "tracking ref {} points to {}, but rev_map r{revision} points to {mapped_oid}",
            target.mapping_ref,
            ref_oid.trim()
        ));
    }
    let message = git.commit_message(&mapped_oid)?;
    let footer = message
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| "imported commit has no git-svn-id footer".to_string())?;
    let identity = GitSvnId::parse(footer.trim_end_matches('\r'))?;
    if identity.url != expected.footer_url
        || identity.uuid != expected.footer_uuid
        || identity.revision != revision
    {
        return Err(format!(
            "imported git-svn-id does not match expected {}@{revision} {}",
            expected.footer_url, expected.footer_uuid
        ));
    }
    let mut imported_tree = tree_map(git.tree_files(&mapped_oid)?);
    for plan in expected.plans {
        canonicalize_tree_keywords(&mut imported_tree, plan);
        if plan.git_commit == expected.git_oid {
            break;
        }
    }
    if imported_tree != *expected.tree {
        let mismatch = expected
            .tree
            .iter()
            .find(|(path, expected)| imported_tree.get(*path) != Some(*expected))
            .map(|(path, expected)| {
                let actual = imported_tree.get(path);
                format!(
                    "{path} (expected mode {} and {} bytes, imported {})",
                    expected.mode,
                    expected.content.len(),
                    actual.map_or_else(
                        || "missing".to_string(),
                        |file| format!("mode {} and {} bytes", file.mode, file.content.len())
                    )
                )
            })
            .or_else(|| {
                imported_tree
                    .keys()
                    .find(|path| !expected.tree.contains_key(*path))
                    .map(|path| format!("unexpected path {path}"))
            })
            .unwrap_or_else(|| "<unknown>".to_string());
        return Err(format!(
            "imported tree does not match the dcommit plan projection at {mismatch}"
        ));
    }
    Ok(mapped_oid)
}

pub(super) fn projected_tree_for_entry(
    git: &GitCli,
    original_base_oid: &str,
    plans: &[DcommitPlan],
    git_oid: &str,
) -> Result<std::collections::BTreeMap<String, crate::git::GitTreeFile>, String> {
    let mut tree = tree_map(git.tree_files(original_base_oid)?);
    for plan in plans {
        apply_plan_to_tree(&mut tree, plan);
        if plan.git_commit == git_oid {
            return Ok(tree);
        }
    }
    Err(format!(
        "dcommit plan queue has no projection for Git commit {git_oid}"
    ))
}
