use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::dcommit::diff_planner::normalize_commit_path;
use crate::svn::editor::CommitEditor;

use super::*;

fn svn_target(path: &str) -> String {
    crate::svn::target_without_peg_revision(path)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorState {
    Open,
    Closed,
    Aborted,
}

/// Applies a typed dcommit plan to an SVN CLI working copy.
pub(super) struct WorkingCopyPlanEditor<'a> {
    wc: PathBuf,
    svn_options: &'a DcommitSvnOptions,
    message: String,
    expected_base: u32,
    state: EditorState,
    pending_adds: BTreeSet<String>,
}

impl<'a> WorkingCopyPlanEditor<'a> {
    pub(super) fn new(
        wc: impl Into<PathBuf>,
        svn_options: &'a DcommitSvnOptions,
        message: impl Into<String>,
        expected_base: u32,
    ) -> Self {
        Self {
            wc: wc.into(),
            svn_options,
            message: message.into(),
            expected_base,
            state: EditorState::Open,
            pending_adds: BTreeSet::new(),
        }
    }

    fn require_open(&self) -> Result<(), String> {
        match self.state {
            EditorState::Open => Ok(()),
            EditorState::Closed => Err("working-copy commit editor is already closed".to_string()),
            EditorState::Aborted => Err("working-copy commit editor was aborted".to_string()),
        }
    }

    fn path(&self, path: &str) -> Result<(String, PathBuf), String> {
        self.require_open()?;
        let path = normalize_commit_path(path)?;
        Ok((path.clone(), self.wc.join(path)))
    }

    fn property_target(&self, path: &str) -> Result<String, String> {
        self.require_open()?;
        if path.is_empty() {
            Ok(".".to_string())
        } else {
            normalize_commit_path(path)
        }
    }

    fn change_prop(&self, path: &str, name: &str, value: Option<&str>) -> Result<(), String> {
        let target = self.property_target(path)?;
        let mut args = match value {
            Some(value) => vec![
                "propset".to_string(),
                "--non-interactive".to_string(),
                name.to_string(),
                value.to_string(),
            ],
            None => vec![
                "propdel".to_string(),
                "--non-interactive".to_string(),
                name.to_string(),
            ],
        };
        args.push(svn_target(&target));
        run_svn(Some(&self.wc), self.svn_options, &args)
    }

    fn verify_copy_source(&self, source_path: &str, revision: u32) -> Result<String, String> {
        self.require_open()?;
        if revision != self.expected_base {
            return Err(format!(
                "copy source revision r{revision} does not match working-copy base r{}",
                self.expected_base
            ));
        }
        normalize_commit_path(source_path)
    }

    fn write_file(&self, path: &Path, content: &[u8]) -> Result<(), String> {
        if std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            #[cfg(unix)]
            if let Some(target) = content.strip_prefix(b"link ") {
                use std::os::unix::fs::symlink;

                let target = std::str::from_utf8(target)
                    .map_err(|_| format!("symlink target for {} is not UTF-8", path.display()))?;
                std::fs::remove_file(path)
                    .map_err(|error| format!("failed to replace {}: {error}", path.display()))?;
                return symlink(target, path).map_err(|error| {
                    format!("failed to create symlink {}: {error}", path.display())
                });
            }
            std::fs::remove_file(path)
                .map_err(|error| format!("failed to replace {}: {error}", path.display()))?;
        }
        make_writable(path)?;
        std::fs::write(path, content)
            .map_err(|error| format!("failed to write {}: {error}", path.display()))
    }

    #[cfg(unix)]
    fn replace_special_kind(
        &self,
        path: &str,
        special: bool,
        pending_add: bool,
    ) -> Result<(), String> {
        use std::os::unix::fs::symlink;

        let (path, target) = self.path(path)?;
        let content = std::fs::read(&target)
            .map_err(|error| format!("failed to read {}: {error}", target.display()))?;
        if !pending_add {
            run_svn(
                Some(&self.wc),
                self.svn_options,
                &[
                    "delete".to_string(),
                    "--keep-local".to_string(),
                    svn_target(&path),
                ],
            )?;
        }
        if std::fs::symlink_metadata(&target).is_ok() {
            std::fs::remove_file(&target)
                .map_err(|error| format!("failed to replace {}: {error}", target.display()))?;
        }
        if special {
            let link_target = content
                .strip_prefix(b"link ")
                .ok_or_else(|| format!("svn:special content for {path} lacks link prefix"))?;
            let link_target = std::str::from_utf8(link_target)
                .map_err(|_| format!("svn:special target for {path} is not UTF-8"))?;
            symlink(link_target, &target).map_err(|error| {
                format!("failed to create symlink {}: {error}", target.display())
            })?;
        } else {
            std::fs::write(&target, content)
                .map_err(|error| format!("failed to write {}: {error}", target.display()))?;
        }
        run_svn(
            Some(&self.wc),
            self.svn_options,
            &["add".to_string(), svn_target(&path)],
        )?;
        if !special {
            run_svn(
                Some(&self.wc),
                self.svn_options,
                &[
                    "propdel".to_string(),
                    "--non-interactive".to_string(),
                    "svn:special".to_string(),
                    svn_target(&path),
                ],
            )?;
        }
        Ok(())
    }
}

fn make_writable(path: &Path) -> Result<(), String> {
    let Ok(metadata) = std::fs::metadata(path) else {
        return Ok(());
    };
    let mut permissions = metadata.permissions();
    if !permissions.readonly() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(permissions.mode() | 0o200);
    }
    #[cfg(not(unix))]
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(path, permissions)
        .map_err(|error| format!("failed to make {} writable: {error}", path.display()))
}

impl CommitEditor for WorkingCopyPlanEditor<'_> {
    fn ensure_path(&mut self, path: &str) -> Result<(), String> {
        let (path, target) = self.path(path)?;
        if target.exists() {
            return Ok(());
        }
        std::fs::create_dir_all(&target)
            .map_err(|error| format!("failed to create {}: {error}", target.display()))?;
        run_svn(
            Some(&self.wc),
            self.svn_options,
            &[
                "add".to_string(),
                "--parents".to_string(),
                svn_target(&path),
            ],
        )
    }

    fn add_file(&mut self, path: &str, content: &[u8]) -> Result<(), String> {
        let (path, target) = self.path(path)?;
        self.write_file(&target, content)?;
        self.pending_adds.insert(path);
        Ok(())
    }

    fn open_file(&mut self, path: &str, content: &[u8]) -> Result<(), String> {
        let (_, target) = self.path(path)?;
        self.write_file(&target, content)
    }

    fn delete_entry(&mut self, path: &str) -> Result<(), String> {
        let (path, _) = self.path(path)?;
        run_svn(
            Some(&self.wc),
            self.svn_options,
            &["delete".to_string(), svn_target(&path)],
        )
    }

    fn copy_file(
        &mut self,
        source_path: &str,
        source_revision: u32,
        path: &str,
    ) -> Result<(), String> {
        let source_path = self.verify_copy_source(source_path, source_revision)?;
        let (path, _) = self.path(path)?;
        run_svn(
            Some(&self.wc),
            self.svn_options,
            &[
                "copy".to_string(),
                svn_target(&source_path),
                svn_target(&path),
            ],
        )
    }

    fn move_entry(
        &mut self,
        source_path: &str,
        source_revision: u32,
        path: &str,
    ) -> Result<(), String> {
        let source_path = self.verify_copy_source(source_path, source_revision)?;
        let (path, _) = self.path(path)?;
        run_svn(
            Some(&self.wc),
            self.svn_options,
            &[
                "move".to_string(),
                svn_target(&source_path),
                svn_target(&path),
            ],
        )
    }

    fn change_file_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        let pending_add = self.pending_adds.remove(path);
        #[cfg(unix)]
        if name == "svn:special" {
            return self.replace_special_kind(path, value.is_some(), pending_add);
        }
        if pending_add {
            run_svn(
                Some(&self.wc),
                self.svn_options,
                &["add".to_string(), "--parents".to_string(), svn_target(path)],
            )?;
        }
        self.change_prop(path, name, value)
    }

    fn change_directory_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        self.change_prop(path, name, value)
    }

    fn close_edit(&mut self) -> Result<u32, String> {
        self.require_open()?;
        for path in std::mem::take(&mut self.pending_adds) {
            run_svn(
                Some(&self.wc),
                self.svn_options,
                &[
                    "add".to_string(),
                    "--parents".to_string(),
                    svn_target(&path),
                ],
            )?;
        }
        let revision = svn_commit(&self.wc, &self.message, self.svn_options)?;
        self.state = EditorState::Closed;
        Ok(revision)
    }

    fn abort_edit(&mut self) -> Result<(), String> {
        self.require_open()?;
        self.state = EditorState::Aborted;
        run_svn(
            Some(&self.wc),
            self.svn_options,
            &[
                "revert".to_string(),
                "--recursive".to_string(),
                ".".to_string(),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_paths_outside_the_working_copy_before_running_svn() {
        let options = DcommitSvnOptions::default();
        let mut editor = WorkingCopyPlanEditor::new("missing", &options, "message", 7);

        let error = editor.add_file("../escape", b"content").unwrap_err();

        assert!(error.contains("outside commit root"), "{error}");
    }

    #[test]
    fn rejects_copy_revision_other_than_the_checked_out_base() {
        let options = DcommitSvnOptions::default();
        let mut editor = WorkingCopyPlanEditor::new("missing", &options, "message", 7);

        let error = editor.copy_file("old.txt", 6, "new.txt").unwrap_err();

        assert_eq!(
            error,
            "copy source revision r6 does not match working-copy base r7"
        );
    }

    #[test]
    fn closed_and_aborted_states_are_terminal() {
        let options = DcommitSvnOptions::default();
        let mut closed = WorkingCopyPlanEditor::new("missing", &options, "message", 7);
        closed.state = EditorState::Closed;
        assert_eq!(
            closed.close_edit().unwrap_err(),
            "working-copy commit editor is already closed"
        );

        let mut aborted = WorkingCopyPlanEditor::new("missing", &options, "message", 7);
        aborted.state = EditorState::Aborted;
        assert_eq!(
            aborted.close_edit().unwrap_err(),
            "working-copy commit editor was aborted"
        );
    }

    #[test]
    fn overwrites_read_only_working_copy_files() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("needs-lock.txt");
        std::fs::write(&path, "old\n").unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).unwrap();
        let options = DcommitSvnOptions::default();
        let mut editor = WorkingCopyPlanEditor::new(temp.path(), &options, "message", 7);

        editor.open_file("needs-lock.txt", b"new\n").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"new\n");
    }
}

pub(super) struct FileSvnDcommit<'a> {
    pub(super) git: &'a GitCli,
    pub(super) svn_root_url: &'a str,
    pub(super) svn_path: &'a str,
    pub(super) uuid: &'a str,
    pub(super) source_refname: &'a str,
    pub(super) mapping_ref: &'a str,
    pub(super) no_rebase: bool,
    pub(super) mergeinfo: Option<&'a str>,
    pub(super) svn_options: DcommitSvnOptions,
    pub(super) post_commit_fetch_shared: crate::cli::SharedFetchArgs,
    pub(super) post_commit_fetch_config: crate::config::SvnRemoteConfig,
    pub(super) remote_id: &'a str,
    pub(super) rev_map_path: &'a Path,
    pub(super) expected_footer_url: String,
    pub(super) expected_footer_uuid: String,
    pub(super) commit_url_override: bool,
    pub(super) adopt_revision: Option<u64>,
}

pub(super) fn dcommit_file_svn(
    ctx: FileSvnDcommit<'_>,
    commits: Vec<GitCommitSummary>,
    active: Option<LocatedJournal>,
) -> Result<String, GitSvnError> {
    if commits.is_empty() && active.is_none() {
        return Ok("No local commits to dcommit.\n".to_string());
    }

    let target_url = svn_checkout_url(ctx.svn_root_url, ctx.svn_path);
    validate_svn_repository_uuid(&target_url, ctx.uuid, &ctx.svn_options)?;
    let target = DcommitTargetIdentity {
        remote_id: ctx.remote_id.to_string(),
        repository_root_url: ctx.svn_root_url.to_string(),
        repository_uuid: ctx.uuid.to_string(),
        mapping_ref: ctx.mapping_ref.to_string(),
        rev_map_path: ctx.rev_map_path.to_string_lossy().into_owned(),
        commit_url: target_url.clone(),
    };
    let authors_file_bytes = ctx
        .post_commit_fetch_config
        .authors_file
        .as_deref()
        .map(|path| {
            std::fs::read(path)
                .map_err(|error| format!("failed to read authors file {path}: {error}"))
        })
        .transpose()?;
    let fetch_intent = RecoveryFetchIntent {
        authors_file: ctx
            .post_commit_fetch_config
            .authors_file
            .as_deref()
            .map(Path::new),
        authors_file_bytes: authors_file_bytes.as_deref(),
        authors_prog: ctx.post_commit_fetch_config.authors_prog.as_deref(),
        ignore_paths: ctx.post_commit_fetch_config.ignore_paths.as_deref(),
        include_paths: ctx.post_commit_fetch_config.include_paths.as_deref(),
        ignore_refs: ctx.post_commit_fetch_config.ignore_refs.as_deref(),
        localtime: ctx.post_commit_fetch_config.localtime,
        no_metadata: ctx.post_commit_fetch_config.no_metadata,
        rewrite_root: ctx.post_commit_fetch_config.rewrite_root.as_deref(),
        rewrite_uuid: ctx.post_commit_fetch_config.rewrite_uuid.as_deref(),
        preserve_empty_dirs: ctx.post_commit_fetch_config.preserve_empty_dirs,
        placeholder_filename: ctx
            .post_commit_fetch_config
            .preserve_empty_dirs
            .then_some(ctx.post_commit_fetch_config.placeholder_filename.as_str()),
    };
    let recovery_fingerprint_input = RecoveryFingerprintInput {
        target: &target,
        commit_url_override: ctx.commit_url_override,
        username: ctx.svn_options.username.as_deref(),
        config_dir: ctx.svn_options.config_dir.as_deref().map(Path::new),
        no_auth_cache: ctx.svn_options.no_auth_cache,
        no_rebase: ctx.no_rebase,
        mergeinfo: ctx.mergeinfo,
        fetch: fetch_intent,
    };
    let config_fingerprint = recovery_config_fingerprint(recovery_fingerprint_input);
    let legacy_config_fingerprint_v2 =
        crate::dcommit::fingerprint::legacy_recovery_config_fingerprint_v2(
            recovery_fingerprint_input,
        );
    let legacy_config_fingerprint_v3 =
        crate::dcommit::fingerprint::legacy_recovery_config_fingerprint_v3(
            recovery_fingerprint_input,
        );
    let new_base = active
        .is_none()
        .then(|| {
            validate_new_dcommit_base(
                ctx.git,
                ctx.mapping_ref,
                ctx.rev_map_path,
                &target_url,
                &ctx.svn_options,
            )
        })
        .transpose()?;
    let plan_chain = if let Some(located) = &active {
        located
            .journal
            .entries
            .iter()
            .map(|entry| {
                ctx.git
                    .rev_parse(&format!("{}^", entry.git_oid))
                    .map(|base| (base.trim().to_string(), entry.git_oid.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        new_plan_chain(ctx.source_refname, &commits)
    };
    let original_base_revision = match &active {
        Some(located) => located.journal.original_base_revision,
        None => {
            new_base
                .as_ref()
                .expect("new dcommit base must be validated")
                .0
        }
    };
    let original_base_oid = match &active {
        Some(located) => located.journal.original_base_oid.clone(),
        None => new_base.expect("new dcommit base must be validated").1,
    };
    let plans = build_dcommit_plans(
        &DcommitPlanningContext {
            git: ctx.git,
            repository_root: ctx.svn_root_url,
            repository_uuid: ctx.uuid,
            mapping_ref: ctx.mapping_ref,
            mergeinfo: ctx.mergeinfo,
        },
        &target_url,
        original_base_revision,
        &plan_chain,
        active.is_none(),
    )?;
    if plans.is_empty() && active.is_none() {
        return finish_noop_dcommit(ctx.git, ctx.mapping_ref, ctx.no_rebase)
            .map_err(GitSvnError::from);
    }
    let original_head = match &active {
        Some(located) => located.journal.original_head.clone(),
        None => plans
            .last()
            .expect("new dcommit plan queue must be nonempty")
            .git_commit
            .clone(),
    };
    let mut prepared = build_prepared_dcommit(PreparedDcommitRequest {
        target: target.clone(),
        original_base_revision,
        original_base_oid,
        original_head,
        no_rebase: ctx.no_rebase,
        config_fingerprint: config_fingerprint.clone(),
        plans,
    })
    .map_err(|error| error.to_string())?;
    let journal_directory = if let Some(located) = active {
        if located.journal.target != target {
            return Err(GitSvnError::metadata_corruption(
                "unfinished dcommit journal target does not match the resolved target".to_string(),
            ));
        }
        let mut journal = located.journal;
        reconcile_recovery_config_fingerprint(
            &mut journal.config_fingerprint,
            &config_fingerprint,
            &[&legacy_config_fingerprint_v3, &legacy_config_fingerprint_v2],
        )?;
        prepared.journal = journal;
        located.directory
    } else {
        ctx.rev_map_path
            .parent()
            .ok_or_else(|| "rev_map path has no metadata directory".to_string())?
            .join("dcommit-journal")
    };

    let temp = TempCheckout::new()?;
    run_svn(
        Some(&temp.root),
        &ctx.svn_options,
        &[
            "checkout".to_string(),
            "--quiet".to_string(),
            crate::svn::target_without_peg_revision(&target_url),
            "wc".to_string(),
        ],
    )?;
    let sink = WorkingCopyCommitSink {
        wc: &temp.wc,
        options: &ctx.svn_options,
        git: ctx.git,
        rev_map_path: ctx.rev_map_path,
    };
    let post_submit = FileSvnPostSubmit {
        git: ctx.git,
        original_base_oid: prepared.journal.original_base_oid.clone(),
        plans: prepared.plans.clone(),
        fetch_config: ctx.post_commit_fetch_config,
        rebase_shared: ctx.post_commit_fetch_shared.clone(),
        fetch_shared: ctx.post_commit_fetch_shared,
        expected_footer_url: ctx.expected_footer_url,
        expected_footer_uuid: ctx.expected_footer_uuid,
    };
    let persistence = JournalStorePersistence::new(JournalStore::new(journal_directory))
        .map_err(|error| error.to_string())?;
    let mut coordinator = Coordinator::new(sink, post_submit, persistence);
    if let Some(revision) = ctx.adopt_revision {
        coordinator
            .adopt_in_flight(&mut prepared, revision)
            .map_err(GitSvnError::from)?;
    }
    coordinator.run(&mut prepared).map_err(GitSvnError::from)?;

    let mut out = format!(
        "Committed {} local Git commit(s)\n",
        prepared.journal.entries.len()
    );
    for entry in &prepared.journal.entries {
        let revision = match entry.state {
            EntryState::Submitted { svn_revision }
            | EntryState::FetchedVerified { svn_revision, .. } => svn_revision,
            _ => {
                return Err(GitSvnError::metadata_corruption(
                    "completed dcommit journal entry has no SVN revision",
                ));
            }
        };
        let label = commits
            .iter()
            .find(|commit| commit.id == entry.git_oid)
            .map_or_else(
                || entry.git_oid.chars().take(12).collect::<String>(),
                |commit| format!("{} {}", commit.short_id, commit.subject),
            );
        out.push_str(&format!("Committed {label} as r{revision}\n"));
    }
    if ctx.no_rebase {
        out.push_str("Skipped rebase (--no-rebase).\n");
    } else {
        out.push_str("Rebased onto tracked SVN ref.\n");
    }
    Ok(out)
}

pub(super) struct WorkingCopyCommitSink<'a> {
    wc: &'a Path,
    options: &'a DcommitSvnOptions,
    git: &'a GitCli,
    rev_map_path: &'a Path,
}

impl CommitSink for WorkingCopyCommitSink<'_> {
    fn remote_head(&mut self, target: &DcommitTargetIdentity) -> Result<RemoteHead, String> {
        let revision = svn_last_changed_revision(&target.commit_url, self.options)?;
        let tracking_oid = RevMap::open_existing(self.rev_map_path, self.git.object_format()?)?
            .max_record(true)?
            .ok_or_else(|| "dcommit target rev_map is empty".to_string())?
            .object_id_hex;
        Ok(RemoteHead {
            revision,
            tracking_oid,
        })
    }

    fn submit(&mut self, plan: &DcommitPlan, expected_base_revision: u64) -> Result<u64, String> {
        let expected_base = u32::try_from(expected_base_revision)
            .map_err(|_| "dcommit expected base revision exceeds u32".to_string())?;
        if plan.base_revision != expected_base {
            return Err(format!(
                "prepared plan base r{} does not match coordinator base r{expected_base}",
                plan.base_revision
            ));
        }
        let mut editor =
            WorkingCopyPlanEditor::new(self.wc, self.options, &plan.message, expected_base);
        SvnCommitEditor::new(PropertyMapper)
            .apply_plan(&mut editor, plan)
            .map(u64::from)
    }
}

pub(super) fn svn_commit(
    wc: &Path,
    message: &str,
    svn_options: &DcommitSvnOptions,
) -> Result<u32, String> {
    let output = run_svn_output(
        Some(wc),
        svn_options,
        &["commit".to_string(), "-m".to_string(), message.to_string()],
    )?;
    parse_committed_revision(&output)
}

pub(super) fn parse_committed_revision(output: &str) -> Result<u32, String> {
    output
        .split(|c: char| !c.is_ascii_digit())
        .rfind(|part| !part.is_empty())
        .ok_or_else(|| format!("svn commit output did not include a revision: {output}"))?
        .parse()
        .map_err(|e| format!("invalid svn commit revision: {e}"))
}

#[derive(Debug, Clone, Default)]
pub(super) struct DcommitSvnOptions {
    config_dir: Option<String>,
    username: Option<String>,
    password: Option<String>,
    no_auth_cache: bool,
}

impl DcommitSvnOptions {
    pub(super) fn command_args(&self, args: &[String]) -> Vec<String> {
        let mut command_args = vec!["--non-interactive".to_string()];
        if let Some(config_dir) = &self.config_dir {
            command_args.push("--config-dir".to_string());
            command_args.push(config_dir.clone());
        }
        if let Some(username) = &self.username {
            command_args.push("--username".to_string());
            command_args.push(username.clone());
        }
        if let Some(password) = &self.password {
            command_args.push("--password".to_string());
            command_args.push(password.clone());
        }
        if self.no_auth_cache {
            command_args.push("--no-auth-cache".to_string());
        }
        command_args.extend(args.iter().cloned());
        command_args
    }
}

pub(super) fn dcommit_svn_options(
    persisted_username: Option<&str>,
    persisted_config_dir: Option<&str>,
    persisted_no_auth_cache: bool,
    persisted_password: Option<&str>,
    shared: &crate::cli::SharedFetchArgs,
) -> DcommitSvnOptions {
    DcommitSvnOptions {
        config_dir: shared
            .config_dir
            .clone()
            .or_else(|| persisted_config_dir.map(|value| value.to_string())),
        username: shared
            .username
            .clone()
            .or_else(|| persisted_username.map(|value| value.to_string())),
        password: shared
            .password
            .clone()
            .or_else(|| persisted_password.map(|value| value.to_string())),
        no_auth_cache: persisted_no_auth_cache || shared.no_auth_cache,
    }
}

pub(super) fn resolved_dcommit_svn_options(
    tracked: &crate::commands::resolver::TrackedSvn,
    args: &DcommitArgs,
    target_url: &str,
) -> Result<DcommitSvnOptions, String> {
    let mut options = dcommit_svn_options(
        tracked.config.username.as_deref(),
        tracked.config.config_dir.as_deref(),
        tracked.config.no_auth_cache,
        None,
        &args.shared,
    );
    if options.password.is_none()
        && matches!(
            crate::path_url::svn_url_profile(target_url),
            crate::path_url::SvnUrlProfile::Svn
                | crate::path_url::SvnUrlProfile::Http
                | crate::path_url::SvnUrlProfile::Https
        )
        && let Some(credentials) = prompted_credentials(
            target_url,
            options.username.as_deref(),
            options.config_dir.as_deref(),
            options.no_auth_cache,
            AuthOperation::Write,
        )?
    {
        options.username = Some(credentials.username);
        options.password = Some(credentials.password);
    }
    if target_url.starts_with("file://") {
        options.username = None;
        options.password = None;
    }
    Ok(options)
}

pub(super) fn run_svn(
    cwd: Option<&Path>,
    options: &DcommitSvnOptions,
    args: &[String],
) -> Result<(), String> {
    run_svn_output(cwd, options, args).map(|_| ())
}

pub(super) fn run_svn_output(
    cwd: Option<&Path>,
    options: &DcommitSvnOptions,
    args: &[String],
) -> Result<String, String> {
    let mut command = Command::new("svn");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let command_args = options.command_args(args);
    let output = command
        .args(command_args)
        .output()
        .map_err(|e| format!("svn failed to start: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("svn exited with status {}", output.status)
        } else {
            stderr
        })
    }
}

pub(super) struct TempCheckout {
    root: PathBuf,
    wc: PathBuf,
}

impl TempCheckout {
    fn new() -> Result<Self, String> {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "git-svn-rs-dcommit-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let root = strip_windows_verbatim_prefix(root.canonicalize().map_err(|e| e.to_string())?);
        let wc = root.join("wc");
        Ok(Self { root, wc })
    }
}

impl Drop for TempCheckout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub(super) fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

pub(super) fn strip_windows_verbatim_prefix(path: PathBuf) -> PathBuf {
    let raw = path.to_string_lossy();
    if let Some(path) = raw.strip_prefix(r"\\?\") {
        PathBuf::from(path)
    } else {
        path
    }
}
