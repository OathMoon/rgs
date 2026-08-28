use super::*;

pub(super) fn import_revisions_for_mapping(
    context: &MappingImportContext<'_>,
    mapping: &RefMapping,
    revisions: &[RevisionEvent],
) -> Result<ImportSummary, String> {
    let MappingImportContext {
        git,
        config,
        uuid,
        all_mappings,
        runtime,
    } = context;
    let strip_prefix = strip_prefix_for(config, &mapping.svn_path);
    let mut stream = FastImportStream::new();
    let mut imported_revisions = Vec::new();
    let max_imported_revision = max_imported_revision(git, &mapping.git_ref, uuid)?;
    let existing_parent_ref = git
        .rev_parse(&mapping.git_ref)
        .ok()
        .map(|commit| commit.trim().to_string());
    let authors = runtime.authors(config)?;
    let metadata_uuid = config.metadata_uuid(uuid)?;
    let staging_ref = next_import_staging_ref();

    for revision in revisions {
        if revision.revision <= max_imported_revision {
            continue;
        }
        let changes = changes_for_revision(revision, &strip_prefix, config, runtime)?;
        if changes.is_empty()
            && !imports_initial_mapping_root(
                revision,
                &mapping.svn_path,
                existing_parent_ref.is_none(),
            )
        {
            continue;
        }

        imported_revisions.push(revision.revision);
        let first_parent_ref = if imported_revisions.len() == 1 {
            if existing_parent_ref.is_some() {
                existing_parent_ref.clone()
            } else {
                copy_from_parent_ref(git, uuid, mapping, all_mappings, revision)?
            }
        } else {
            None
        };
        let timestamp = svn_git_timestamp(&revision.timestamp, config.localtime)?;
        stream = stream.commit(&FastImportCommit {
            mark: imported_revisions.len() as u32,
            refname: staging_ref.clone(),
            author: author_ident(&revision.author, metadata_uuid, Some(authors))?,
            committer: author_ident(&revision.author, metadata_uuid, Some(authors))?,
            timestamp: timestamp.seconds,
            timezone_offset: timestamp.offset,
            message: commit_message(config, revision, uuid, &mapping.svn_path)?,
            parent_mark: (imported_revisions.len() > 1)
                .then_some(imported_revisions.len() as u32 - 1),
            parent_ref: first_parent_ref,
            changes,
        });
    }

    if imported_revisions.is_empty() {
        return Ok(ImportSummary { imported_revisions });
    }

    git.fast_import(&stream.finish())?;
    publish_imported_revisions(
        git,
        &mapping.git_ref,
        uuid,
        existing_parent_ref.as_deref(),
        &staging_ref,
        ImportedRevisionMaps {
            transport: &imported_revisions,
            source: None,
        },
        None,
    )?;

    Ok(ImportSummary { imported_revisions })
}

pub(super) fn import_ra_revisions_for_mapping(
    context: &MappingImportContext<'_>,
    mapping: &RefMapping,
    revisions: &[RevisionEvent],
    session: &impl RaSession,
) -> Result<ImportSummary, String> {
    let MappingImportContext {
        git,
        config,
        uuid,
        all_mappings,
        runtime,
    } = context;
    let strip_prefix = strip_prefix_for(config, &mapping.svn_path);
    let mut stream = FastImportStream::new();
    let mut imported_revisions = Vec::new();
    let mut source_revisions = Vec::new();
    let mut unhandled_revisions = Vec::new();
    let max_imported_revision = max_imported_revision(git, &mapping.git_ref, uuid)?;
    let mut owned_placeholders = if config.preserve_empty_dirs {
        placeholder_ownership(git, mapping, max_imported_revision)?
    } else {
        BTreeSet::new()
    };
    let existing_parent_ref = git
        .rev_parse(&mapping.git_ref)
        .ok()
        .map(|commit| commit.trim().to_string());
    let authors = runtime.authors(config)?;
    let staging_ref = next_import_staging_ref();

    for revision in revisions {
        if revision.revision <= max_imported_revision {
            continue;
        }

        let Some(identity) = import_revision_identity(config, uuid, mapping, revision, session)?
        else {
            continue;
        };

        let parent_mark =
            (!imported_revisions.is_empty()).then_some(imported_revisions.len() as u32);
        let copy_parent = if imported_revisions.is_empty() && existing_parent_ref.is_none() {
            copy_parent_source(git, uuid, mapping, all_mappings, revision)?
        } else {
            None
        };
        let parent_ref = if imported_revisions.is_empty() {
            existing_parent_ref
                .clone()
                .or_else(|| copy_parent.as_ref().map(|parent| parent.commit.clone()))
        } else {
            None
        };
        let timestamp = svn_git_timestamp(&revision.timestamp, config.localtime)?;
        let plan = FetchCommitPlan {
            mark: imported_revisions.len() as u32 + 1,
            refname: staging_ref.clone(),
            author: author_ident(&revision.author, &identity.uuid, Some(authors))?,
            committer: author_ident(&revision.author, &identity.uuid, Some(authors))?,
            timestamp: timestamp.seconds,
            timezone_offset: timestamp.offset,
            message: commit_message_with_identity(config, revision, &identity)?,
            parent_mark,
            parent_ref,
        };
        let mut editor = if let Some(copy_parent) = copy_parent {
            let mut entries =
                prefixed_tree_entries(git, &copy_parent.commit, &copy_parent.svn_path)?;
            if !mapping.svn_path.trim_matches('/').is_empty() {
                entries.extend(prefixed_tree_entries(git, &copy_parent.commit, "")?);
            }
            SvnFetchEditor::with_base_tree(plan, entries)
        } else if existing_parent_ref.is_some() && imported_revisions.is_empty() {
            SvnFetchEditor::from_git_ref(git, plan, &mapping.git_ref)?
        } else {
            SvnFetchEditor::new(plan)
        }
        .with_path_prefix(&mapping.svn_path)
        .with_owned_placeholders(owned_placeholders.clone());

        let filters = runtime.filters(config)?;
        let mut filtered_editor = FilteredFetchEditor {
            inner: &mut editor,
            filters,
        };
        let base_revision = imported_revisions
            .last()
            .copied()
            .or((max_imported_revision > 0).then_some(max_imported_revision));
        session.do_update_from(
            &mapping.svn_path,
            UpdateRequest {
                target_revision: revision.revision,
                base_revision,
            },
            &mut filtered_editor,
        )?;
        if config.preserve_empty_dirs {
            editor.reconcile_empty_directories(&config.placeholder_filename)?;
        }
        let result = editor.into_result()?;
        owned_placeholders = result.owned_placeholders;
        let mut commit = result.commit;
        commit.changes = finalize_ra_changes(commit.changes, revision, &strip_prefix, filters)?;
        if commit.changes.iter().any(|change| match change {
            FileChange::Modify { path, .. } | FileChange::Delete { path } => path.is_empty(),
        }) {
            return Err(format!(
                "SVN r{} produced an invalid empty Git path for mapping {}",
                revision.revision, mapping.svn_path
            ));
        }
        if commit.changes.is_empty()
            && result.unhandled.is_empty()
            && !imports_initial_mapping_root(
                revision,
                &mapping.svn_path,
                existing_parent_ref.is_none(),
            )
        {
            continue;
        }

        imported_revisions.push(revision.revision);
        source_revisions.push(identity.source_revision);
        unhandled_revisions.push((revision.revision, result.unhandled));
        stream = stream.commit(&commit);
    }

    if imported_revisions.is_empty() {
        return Ok(ImportSummary { imported_revisions });
    }

    git.fast_import(&stream.finish())?;
    let append = unhandled_append(git, &mapping.git_ref, &unhandled_revisions)?;
    publish_imported_revisions(
        git,
        &mapping.git_ref,
        uuid,
        existing_parent_ref.as_deref(),
        &staging_ref,
        ImportedRevisionMaps {
            transport: &imported_revisions,
            source: config
                .svm_uuid
                .as_deref()
                .zip(Some(source_revisions.as_slice())),
        },
        append,
    )?;

    Ok(ImportSummary { imported_revisions })
}

pub(super) struct FilteredFetchEditor<'a> {
    inner: &'a mut dyn FetchEditor,
    filters: &'a PathFilters,
}

impl FilteredFetchEditor<'_> {
    fn path_is_included(&self, path: &str) -> Result<bool, String> {
        path_is_included(self.filters, path)
    }

    fn included_copy_from<'b>(
        &self,
        copy_from: Option<(&'b str, u32)>,
    ) -> Result<Option<(&'b str, u32)>, String> {
        match copy_from {
            Some((source, revision)) if self.path_is_included(source)? => {
                Ok(Some((source, revision)))
            }
            Some(_) | None => Ok(None),
        }
    }
}

impl FetchEditor for FilteredFetchEditor<'_> {
    fn open_root(&mut self, revision: u32) -> Result<(), String> {
        self.inner.open_root(revision)
    }

    fn add_directory(&mut self, path: &str, copy_from: Option<(&str, u32)>) -> Result<(), String> {
        if !self.path_is_included(path)? {
            return Ok(());
        }
        self.inner
            .add_directory(path, self.included_copy_from(copy_from)?)
    }

    fn add_file(&mut self, path: &str, copy_from: Option<(&str, u32)>) -> Result<(), String> {
        if !self.path_is_included(path)? {
            return Ok(());
        }
        self.inner
            .add_file(path, self.included_copy_from(copy_from)?)
    }

    fn add_file_with_copy_content(
        &mut self,
        path: &str,
        copy_from: (&str, u32),
        content: &[u8],
    ) -> Result<(), String> {
        if !self.path_is_included(path)? {
            return Ok(());
        }
        self.inner
            .add_file_with_copy_content(path, copy_from, content)
    }

    fn delete_entry(&mut self, path: &str, revision: u32) -> Result<(), String> {
        if !self.path_is_included(path)? {
            return Ok(());
        }
        self.inner.delete_entry(path, revision)
    }

    fn change_file_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        if !self.path_is_included(path)? {
            return Ok(());
        }
        self.inner.change_file_prop(path, name, value)
    }

    fn change_file_prop_bytes(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&[u8]>,
    ) -> Result<(), String> {
        if !self.path_is_included(path)? {
            return Ok(());
        }
        self.inner.change_file_prop_bytes(path, name, value)
    }

    fn change_directory_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        if !self.path_is_included(path)? {
            return Ok(());
        }
        self.inner.change_directory_prop(path, name, value)
    }

    fn change_directory_prop_bytes(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&[u8]>,
    ) -> Result<(), String> {
        if !self.path_is_included(path)? {
            return Ok(());
        }
        self.inner.change_directory_prop_bytes(path, name, value)
    }

    fn apply_textdelta(&mut self, path: &str, content: &[u8]) -> Result<(), String> {
        if !self.path_is_included(path)? {
            return Ok(());
        }
        self.inner.apply_textdelta(path, content)
    }

    fn absent_directory(&mut self, path: &str) -> Result<(), String> {
        if !self.path_is_included(path)? {
            return Ok(());
        }
        self.inner.absent_directory(path)
    }

    fn absent_file(&mut self, path: &str) -> Result<(), String> {
        if !self.path_is_included(path)? {
            return Ok(());
        }
        self.inner.absent_file(path)
    }

    fn close_edit(&mut self) -> Result<(), String> {
        self.inner.close_edit()
    }

    fn abort_edit(&mut self) -> Result<(), String> {
        self.inner.abort_edit()
    }
}

pub(super) fn finalize_ra_changes(
    changes: Vec<FileChange>,
    _revision: &RevisionEvent,
    strip_prefix: &str,
    filters: &PathFilters,
) -> Result<Vec<FileChange>, String> {
    let mut filtered = Vec::new();
    for change in changes {
        let git_path = match &change {
            FileChange::Modify { path, .. } | FileChange::Delete { path } => path,
        };
        let svn_path = svn_path_for_git_path(strip_prefix, git_path);
        if path_is_included(filters, &svn_path)? {
            filtered.push(change);
        }
    }

    Ok(filtered)
}

pub(super) fn svn_path_for_git_path(strip_prefix: &str, git_path: &str) -> String {
    let strip_prefix = strip_prefix.trim_matches('/');
    let git_path = git_path.trim_matches('/');
    if strip_prefix.is_empty() {
        git_path.to_string()
    } else if git_path.is_empty() {
        strip_prefix.to_string()
    } else {
        format!("{strip_prefix}/{git_path}")
    }
}

pub(super) fn revisions_for_mapping(
    revisions: &[RevisionEvent],
    svn_path: &str,
) -> Vec<RevisionEvent> {
    let svn_path = svn_path.trim_matches('/');
    revisions
        .iter()
        .filter(|revision| {
            svn_path.is_empty()
                || revision.changed_paths.is_empty()
                || revision.changed_paths.iter().any(|changed_path| {
                    let changed_path = changed_path.path.trim_matches('/');
                    changed_path == svn_path || changed_path.starts_with(&format!("{svn_path}/"))
                })
        })
        .cloned()
        .collect()
}

pub(super) fn imports_initial_mapping_root(
    revision: &RevisionEvent,
    svn_path: &str,
    has_no_existing_parent: bool,
) -> bool {
    has_no_existing_parent
        && revision.changed_paths.iter().any(|changed_path| {
            changed_path.path.trim_matches('/') == svn_path.trim_matches('/')
                && changed_path.kind == NodeKind::Directory
                && matches!(
                    changed_path.action,
                    ChangeAction::Add | ChangeAction::Replace
                )
        })
}

pub(super) fn wildcard_from_exact_match(spec: &GlobSpec, path: &str) -> Option<String> {
    let mut relative = path;
    if !spec.left().is_empty() {
        relative = relative.strip_prefix(spec.left())?.trim_start_matches('/');
    }
    if !spec.right().is_empty() {
        relative = relative.strip_suffix(spec.right())?.trim_end_matches('/');
    }
    (!relative.is_empty()).then(|| relative.to_string())
}

pub(super) fn changes_for_revision(
    revision: &RevisionEvent,
    strip_prefix: &str,
    config: &SvnRemoteConfig,
    runtime: &ImportRuntime,
) -> Result<Vec<FileChange>, String> {
    if revision.changed_paths.is_empty() {
        return Ok(mock_fixture_changes(revision.revision));
    }
    let filters = runtime.filters(config)?;

    let mut file_paths = Vec::new();
    for changed_path in &revision.changed_paths {
        if matches!(
            changed_path.action,
            ChangeAction::Add | ChangeAction::Modify | ChangeAction::Replace
        ) && changed_path.kind == NodeKind::File
            && path_is_included(filters, &changed_path.path)?
            && let Some(path) = import_path(&changed_path.path, strip_prefix)
        {
            file_paths.push(path);
        }
    }

    let mut changes = Vec::new();
    for changed_path in &revision.changed_paths {
        if !path_is_included(filters, &changed_path.path)? {
            continue;
        }
        let Some(path) = import_path(&changed_path.path, strip_prefix) else {
            continue;
        };
        match changed_path.action {
            ChangeAction::Delete
                if changed_path.kind == NodeKind::Directory && config.preserve_empty_dirs =>
            {
                changes.push(FileChange::Delete {
                    path: placeholder_path(&path, config),
                });
            }
            ChangeAction::Delete => changes.push(FileChange::Delete { path }),
            ChangeAction::Add | ChangeAction::Modify | ChangeAction::Replace
                if changed_path.kind == NodeKind::File =>
            {
                changes.push(FileChange::Modify {
                    path,
                    mode: mode_for_change(changed_path),
                    content: content_for_change(changed_path),
                });
            }
            _ => {}
        }
    }

    if config.preserve_empty_dirs {
        for changed_path in &revision.changed_paths {
            if !matches!(
                changed_path.action,
                ChangeAction::Add | ChangeAction::Replace | ChangeAction::Modify
            ) || changed_path.kind != NodeKind::Directory
            {
                continue;
            }
            if !path_is_included(filters, &changed_path.path)? {
                continue;
            }
            let Some(path) = import_path(&changed_path.path, strip_prefix) else {
                continue;
            };
            let child_prefix = format!("{}/", path.trim_end_matches('/'));
            if file_paths
                .iter()
                .any(|file_path| file_path == &path || file_path.starts_with(&child_prefix))
            {
                continue;
            }
            changes.push(FileChange::Modify {
                path: placeholder_path(&path, config),
                mode: "100644".to_string(),
                content: Vec::new(),
            });
        }
    }

    Ok(changes)
}

pub(super) fn path_is_included(filters: &PathFilters, path: &str) -> Result<bool, String> {
    let path = path.trim_matches('/');
    Ok(filters.decide(path)? == FilterDecision::Include)
}

pub(super) fn placeholder_path(path: &str, config: &SvnRemoteConfig) -> String {
    format!(
        "{}/{}",
        path.trim_end_matches('/'),
        config.placeholder_filename.trim_matches('/')
    )
}

pub(super) fn mode_for_change(changed_path: &crate::svn::ChangedPath) -> String {
    if changed_path.kind == NodeKind::Symlink || changed_path.properties.contains_key("svn:special")
    {
        "120000"
    } else if changed_path.properties.contains_key("svn:executable") {
        "100755"
    } else {
        "100644"
    }
    .to_string()
}

pub(super) fn content_for_change(changed_path: &crate::svn::ChangedPath) -> Vec<u8> {
    let content = changed_path.content.clone().unwrap_or_default();
    if changed_path.properties.contains_key("svn:special") && content.starts_with(b"link ") {
        content[5..].to_vec()
    } else {
        content
    }
}

pub(super) fn mock_fixture_changes(revision: u32) -> Vec<FileChange> {
    if revision < 2 {
        return Vec::new();
    }

    vec![FileChange::Modify {
        path: "src/lib.rs".to_string(),
        mode: "100644".to_string(),
        content: b"pub fn answer() -> u8 { 42 }\n".to_vec(),
    }]
}

pub(super) fn import_path(path: &str, strip_prefix: &str) -> Option<String> {
    let path = path.trim_matches('/');
    let relative = if strip_prefix.is_empty() {
        path
    } else {
        path.strip_prefix(strip_prefix)?.trim_start_matches('/')
    };
    (!relative.is_empty()).then(|| relative.to_string())
}

pub(super) struct ImportRevisionIdentity {
    url: String,
    revision: u32,
    uuid: String,
    source_revision: Option<u32>,
}

pub(super) struct ImportedRevisionMaps<'a> {
    pub(super) transport: &'a [u32],
    pub(super) source: Option<(&'a str, &'a [Option<u32>])>,
}

pub(super) fn import_revision_identity(
    config: &SvnRemoteConfig,
    transport_uuid: &str,
    mapping: &RefMapping,
    revision: &RevisionEvent,
    session: &impl RaSession,
) -> Result<Option<ImportRevisionIdentity>, String> {
    let mirror = || -> Result<ImportRevisionIdentity, String> {
        Ok(ImportRevisionIdentity {
            url: config.metadata_url(&mapping.svn_path)?,
            revision: revision.revision,
            uuid: config.metadata_uuid(transport_uuid)?.to_string(),
            source_revision: None,
        })
    };
    if !config.use_svm_props {
        return mirror().map(Some);
    }
    let properties = session.rev_properties(revision.revision)?;
    let Some(value) = properties.get("svm:headrev") else {
        return mirror().map(Some);
    };
    let value = std::str::from_utf8(value)
        .map_err(|_| format!("svm:headrev at r{} is not valid UTF-8", revision.revision))?
        .trim_end_matches(['\r', '\n']);
    let (source_uuid, source_revision) = value
        .split_once(':')
        .ok_or_else(|| format!("invalid svm:headrev at r{}: {value:?}", revision.revision))?;
    crate::config::validate_svm_uuid(source_uuid)?;
    let expected_uuid = config.svm_uuid.as_deref().ok_or_else(|| {
        format!(
            "svn-remote.{} uses useSvmProps but has no validated SVM identity",
            config.name
        )
    })?;
    if source_uuid != expected_uuid {
        return Err(format!(
            "UUID mismatch on SVM path: expected {expected_uuid}, got {source_uuid}"
        ));
    }
    let source_revision = source_revision
        .parse::<u32>()
        .map_err(|_| format!("invalid svm:headrev at r{}: {value:?}", revision.revision))?;
    if source_revision == 0 {
        return Ok(None);
    }
    Ok(Some(ImportRevisionIdentity {
        url: crate::tracking_state::svm_metadata_url(config, &mapping.svn_path)?,
        revision: source_revision,
        uuid: source_uuid.to_string(),
        source_revision: Some(source_revision),
    }))
}

pub(super) fn commit_message_with_identity(
    config: &SvnRemoteConfig,
    revision: &RevisionEvent,
    identity: &ImportRevisionIdentity,
) -> Result<String, String> {
    if config.no_metadata {
        return Ok(revision.message.clone());
    }
    let footer = GitSvnId {
        url: identity.url.clone(),
        revision: identity.revision,
        uuid: identity.uuid.clone(),
    }
    .to_footer();
    Ok(format!("{}\n\n{}\n", revision.message, footer))
}

pub(super) fn commit_message(
    config: &SvnRemoteConfig,
    revision: &RevisionEvent,
    uuid: &str,
    svn_path: &str,
) -> Result<String, String> {
    if config.no_metadata {
        return Ok(revision.message.clone());
    }

    let footer = GitSvnId {
        url: config.metadata_url(svn_path)?,
        revision: revision.revision,
        uuid: config.metadata_uuid(uuid)?.to_string(),
    }
    .to_footer();
    Ok(format!("{}\n\n{}\n", revision.message, footer))
}

pub(super) struct AuthorMapper {
    file: Option<AuthorResolver>,
    prog: Option<String>,
}

pub(super) fn author_mapper(config: &SvnRemoteConfig) -> Result<AuthorMapper, String> {
    let file = if let Some(path) = &config.authors_file {
        let contents = std::fs::read_to_string(path)
            .map_err(|error| format!("failed to read authors file {path}: {error}"))?;
        Some(
            parse_authors_file(&contents)
                .map_err(|error| format!("failed to parse authors file {path}: {error}"))?,
        )
    } else {
        None
    };
    Ok(AuthorMapper {
        file,
        prog: config.authors_prog.clone(),
    })
}

pub(super) fn author_ident(
    author: &str,
    uuid: &str,
    mapper: Option<&AuthorMapper>,
) -> Result<String, String> {
    if let Some(mapped) = mapper
        .and_then(|mapper| mapper.file.as_ref())
        .and_then(|resolver| resolver.resolve(author))
    {
        Ok(format!("{} <{}>", mapped.name, mapped.email))
    } else if let Some(prog) = mapper.and_then(|mapper| mapper.prog.as_ref()) {
        run_authors_prog(prog, author)
    } else {
        Ok(format!("{author} <{author}@{uuid}>"))
    }
}

pub(super) fn run_authors_prog(program: &str, author: &str) -> Result<String, String> {
    let output = Command::new(program)
        .arg(author)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("authors-prog exited with status {}", output.status)
        } else {
            stderr
        });
    }
    let ident = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if ident.is_empty() || !ident.contains('<') || !ident.contains('>') {
        return Err(format!(
            "authors-prog returned invalid identity for {author}"
        ));
    }
    Ok(ident)
}

pub(super) fn strip_prefix_for(config: &SvnRemoteConfig, svn_path: &str) -> String {
    if !svn_path.is_empty() {
        return svn_path.trim_matches('/').to_string();
    }
    if config.url.starts_with("mock://") {
        return config
            .url
            .strip_prefix("mock://")
            .and_then(|rest| rest.split_once('/').map(|(_, path)| path))
            .unwrap_or_default()
            .trim_matches('/')
            .to_string();
    }
    String::new()
}

pub(super) struct GitTimestamp {
    pub(super) seconds: i64,
    pub(super) offset: String,
}

pub(super) fn svn_git_timestamp(value: &str, localtime: bool) -> Result<GitTimestamp, String> {
    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|error| format!("invalid SVN revision date {value:?}: {error}"))?;
    let seconds = parsed.timestamp();
    let offset = if localtime {
        parsed.with_timezone(&Local).format("%z").to_string()
    } else {
        "+0000".to_string()
    };
    Ok(GitTimestamp { seconds, offset })
}
