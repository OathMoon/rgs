use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::PathBuf;
use std::process::Command;

use crate::authors::{AuthorResolver, parse_authors_file};
use crate::config::SvnRemoteConfig;
use crate::fast_import::{FastImportCommit, FastImportStream, FileChange};
use crate::fetch_editor::{FetchCommitPlan, SvnFetchEditor, TreeEntry, UnhandledMetadata};
use crate::filters::{FilterDecision, PathFilters};
use crate::git::GitCli;
use crate::git_svn_id::GitSvnId;
use crate::glob_spec::GlobSpec;
use crate::import_transaction::{
    ImportAppend, ImportPublication, begin_or_resume_batch,
    complete as complete_import_publication, finish_batch_if_complete,
    mark_batch_mapping_completed,
};
use crate::mapping::{MappingKind, RefMapping, sanitize_refname};
use crate::metadata::svn_metadata_dir;
use crate::rev_map::{RevMap, RevMapRecord};
use crate::svn::editor::FetchEditor;
use crate::svn::ra::{RaSession, UpdateRequest};
use crate::svn::{ChangeAction, ChangedPath, NodeKind, RevisionEvent, SvnBackend};
use chrono::{DateTime, Local};
use fancy_regex::Regex;
use flate2::read::GzDecoder;
use std::sync::atomic::{AtomicU64, Ordering};

static IMPORT_STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportOptions {
    pub start_revision: u32,
    pub end_revision: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    pub imported_revisions: Vec<u32>,
}

pub fn import_mock_revisions(
    backend: &impl SvnBackend,
    git: &GitCli,
    config: &SvnRemoteConfig,
    options: ImportOptions,
) -> Result<ImportSummary, String> {
    import_mock_revisions_for_ref(backend, git, config, options, None)
}

pub fn import_mock_revisions_for_ref(
    backend: &impl SvnBackend,
    git: &GitCli,
    config: &SvnRemoteConfig,
    options: ImportOptions,
    selected_ref: Option<&str>,
) -> Result<ImportSummary, String> {
    config.validate_mapping_destinations()?;
    if let Some(window_size) = config.log_window_size {
        return import_mock_revisions_in_windows(
            backend,
            git,
            config,
            options,
            selected_ref,
            window_size,
        );
    }
    crate::import_transaction::recover_pending(git)?;
    finish_batch_if_complete(git)?;
    let end = options.end_revision.unwrap_or(backend.latest_revnum()?);
    if options.start_revision > end {
        return Ok(ImportSummary {
            imported_revisions: Vec::new(),
        });
    }

    let mut revisions = backend.log(options.start_revision, end)?;
    let uuid = backend.uuid()?;
    let mut mappings = concrete_mappings(config, &revisions)?;
    let mut auxiliary_revisions = BTreeMap::new();
    loop {
        let previous_mapping_count = mappings.len();
        let previous_revision_count = revisions.len();
        auxiliary_revisions.extend(ensure_auxiliary_copy_mappings(
            git,
            &revisions,
            &mut mappings,
        )?);
        backfill_mock_copy_sources(backend, git, &uuid, &mut revisions, &mappings)?;
        if mappings.len() == previous_mapping_count && revisions.len() == previous_revision_count {
            break;
        }
    }
    let marker_refnames = scan_marker_refnames(config, selected_ref)?;
    if mappings.is_empty() && marker_refnames.is_empty() {
        return Ok(ImportSummary {
            imported_revisions: Vec::new(),
        });
    }
    let mut all_imported_revisions = Vec::new();
    let selected_mappings = select_and_order_mappings(&mappings, selected_ref, &revisions);
    if selected_mappings.is_empty() && marker_refnames.is_empty() {
        return Ok(ImportSummary {
            imported_revisions: Vec::new(),
        });
    }
    validate_mapping_ref_collisions(&selected_mappings)?;
    let mut batch_refs = selected_mappings
        .iter()
        .map(|mapping| mapping.git_ref.clone())
        .collect::<Vec<_>>();
    batch_refs.extend(marker_refnames.iter().cloned());
    batch_refs.sort();
    batch_refs.dedup();
    validate_ref_storage_collisions(git, &batch_refs)?;
    begin_or_resume_batch(git, &uuid, &batch_refs)?;

    let mut completed_refs = BTreeSet::new();
    for mapping in selected_mappings {
        let mut mapping_revisions = revisions_for_mapping(&revisions, &mapping.svn_path);
        if let Some(start) = auxiliary_revisions.get(&mapping.git_ref) {
            mapping_revisions.retain(|revision| revision.revision >= *start);
        }
        if !mapping_revisions.is_empty() {
            let summary = import_revisions_for_mapping(
                git,
                config,
                &uuid,
                mapping,
                &mappings,
                &mapping_revisions,
            )?;
            all_imported_revisions.extend(summary.imported_revisions);
        }
        if marker_refnames.contains(&mapping.git_ref) {
            publish_scan_marker(git, &mapping.git_ref, &uuid, end)?;
        }
        mark_batch_mapping_completed(git, &mapping.git_ref)?;
        completed_refs.insert(mapping.git_ref.clone());
    }
    for refname in marker_refnames.difference(&completed_refs) {
        publish_scan_marker(git, refname, &uuid, end)?;
        mark_batch_mapping_completed(git, refname)?;
    }
    finish_batch_if_complete(git)?;

    all_imported_revisions.sort_unstable();
    all_imported_revisions.dedup();
    Ok(ImportSummary {
        imported_revisions: all_imported_revisions,
    })
}

pub fn import_ra_revisions(
    session: &impl RaSession,
    git: &GitCli,
    config: &SvnRemoteConfig,
    options: ImportOptions,
) -> Result<ImportSummary, String> {
    import_ra_revisions_for_ref(session, git, config, options, None)
}

pub fn import_ra_revisions_for_ref(
    session: &impl RaSession,
    git: &GitCli,
    config: &SvnRemoteConfig,
    options: ImportOptions,
    selected_ref: Option<&str>,
) -> Result<ImportSummary, String> {
    config.validate_mapping_destinations()?;
    if let Some(window_size) = config.log_window_size {
        return import_ra_revisions_in_windows(
            session,
            git,
            config,
            options,
            selected_ref,
            window_size,
        );
    }
    crate::import_transaction::recover_pending(git)?;
    finish_batch_if_complete(git)?;
    let end = options.end_revision.unwrap_or(session.latest_revnum()?);
    if options.start_revision > end {
        return Ok(ImportSummary {
            imported_revisions: Vec::new(),
        });
    }

    let mut revisions = session.get_log(&[], options.start_revision, end)?;
    expand_ra_wildcard_ancestor_copies(session, config, &mut revisions)?;

    let uuid = session.uuid()?;
    let mut mappings = concrete_mappings(config, &revisions)?;
    let mut auxiliary_revisions = BTreeMap::new();
    loop {
        let previous_mapping_count = mappings.len();
        let previous_revision_count = revisions.len();
        auxiliary_revisions.extend(ensure_auxiliary_copy_mappings(
            git,
            &revisions,
            &mut mappings,
        )?);
        backfill_ra_copy_sources(session, git, &uuid, &mut revisions, &mappings)?;
        if mappings.len() == previous_mapping_count && revisions.len() == previous_revision_count {
            break;
        }
    }
    let marker_refnames = scan_marker_refnames(config, selected_ref)?;
    let mut all_imported_revisions = Vec::new();
    let selected_mappings = select_and_order_mappings(&mappings, selected_ref, &revisions);
    if selected_mappings.is_empty() && marker_refnames.is_empty() {
        return Ok(ImportSummary {
            imported_revisions: Vec::new(),
        });
    }
    validate_mapping_ref_collisions(&selected_mappings)?;
    let mut batch_refs = selected_mappings
        .iter()
        .map(|mapping| mapping.git_ref.clone())
        .collect::<Vec<_>>();
    batch_refs.extend(marker_refnames.iter().cloned());
    batch_refs.sort();
    batch_refs.dedup();
    validate_ref_storage_collisions(git, &batch_refs)?;
    begin_or_resume_batch(git, &uuid, &batch_refs)?;
    let mut completed_refs = BTreeSet::new();
    for mapping in selected_mappings {
        let mut mapping_revisions = revisions_for_mapping(&revisions, &mapping.svn_path);
        if let Some(start) = auxiliary_revisions.get(&mapping.git_ref) {
            mapping_revisions.retain(|revision| revision.revision >= *start);
        }
        if !mapping_revisions.is_empty() {
            let summary = import_ra_revisions_for_mapping(
                git,
                config,
                &uuid,
                mapping,
                &mappings,
                &mapping_revisions,
                session,
            )?;
            all_imported_revisions.extend(summary.imported_revisions);
        }
        if marker_refnames.contains(&mapping.git_ref) {
            publish_scan_marker(git, &mapping.git_ref, &uuid, end)?;
        }
        mark_batch_mapping_completed(git, &mapping.git_ref)?;
        completed_refs.insert(mapping.git_ref.clone());
    }
    for refname in marker_refnames.difference(&completed_refs) {
        publish_scan_marker(git, refname, &uuid, end)?;
        mark_batch_mapping_completed(git, refname)?;
    }
    finish_batch_if_complete(git)?;

    all_imported_revisions.sort_unstable();
    all_imported_revisions.dedup();
    Ok(ImportSummary {
        imported_revisions: all_imported_revisions,
    })
}

fn import_mock_revisions_in_windows(
    backend: &impl SvnBackend,
    git: &GitCli,
    config: &SvnRemoteConfig,
    options: ImportOptions,
    selected_ref: Option<&str>,
    window_size: u32,
) -> Result<ImportSummary, String> {
    let options = ImportOptions {
        end_revision: Some(options.end_revision.unwrap_or(backend.latest_revnum()?)),
        ..options
    };
    import_in_windows(config, options, window_size, |window_config, options| {
        import_mock_revisions_for_ref(backend, git, window_config, options, selected_ref)
    })
}

fn import_ra_revisions_in_windows(
    session: &impl RaSession,
    git: &GitCli,
    config: &SvnRemoteConfig,
    options: ImportOptions,
    selected_ref: Option<&str>,
    window_size: u32,
) -> Result<ImportSummary, String> {
    let options = ImportOptions {
        end_revision: Some(options.end_revision.unwrap_or(session.latest_revnum()?)),
        ..options
    };
    import_in_windows(config, options, window_size, |window_config, options| {
        import_ra_revisions_for_ref(session, git, window_config, options, selected_ref)
    })
}

fn import_in_windows(
    config: &SvnRemoteConfig,
    options: ImportOptions,
    window_size: u32,
    mut import_window: impl FnMut(&SvnRemoteConfig, ImportOptions) -> Result<ImportSummary, String>,
) -> Result<ImportSummary, String> {
    if window_size == 0 {
        return Err("--log-window-size must be greater than zero".to_string());
    }
    let end = options.end_revision.unwrap_or(options.start_revision);
    if options.start_revision > end {
        return Ok(ImportSummary {
            imported_revisions: Vec::new(),
        });
    }

    let mut window_config = config.clone();
    window_config.log_window_size = None;
    let mut start = options.start_revision;
    let mut imported_revisions = Vec::new();
    loop {
        let window_end = start.saturating_add(window_size - 1).min(end);
        imported_revisions.extend(
            import_window(
                &window_config,
                ImportOptions {
                    start_revision: start,
                    end_revision: Some(window_end),
                },
            )?
            .imported_revisions,
        );
        if window_end == end {
            break;
        }
        start = window_end + 1;
    }
    imported_revisions.sort_unstable();
    imported_revisions.dedup();
    Ok(ImportSummary { imported_revisions })
}

fn import_revisions_for_mapping(
    git: &GitCli,
    config: &SvnRemoteConfig,
    uuid: &str,
    mapping: &RefMapping,
    all_mappings: &[RefMapping],
    revisions: &[RevisionEvent],
) -> Result<ImportSummary, String> {
    let strip_prefix = strip_prefix_for(config, &mapping.svn_path);
    let mut stream = FastImportStream::new();
    let mut imported_revisions = Vec::new();
    let max_imported_revision = max_imported_revision(git, &mapping.git_ref, uuid)?;
    let existing_parent_ref = git
        .rev_parse(&mapping.git_ref)
        .ok()
        .map(|commit| commit.trim().to_string());
    let authors = author_mapper(config)?;
    let staging_ref = next_import_staging_ref();

    for revision in revisions {
        if revision.revision <= max_imported_revision {
            continue;
        }
        let changes = changes_for_revision(revision, &strip_prefix, config)?;
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
            author: author_ident(&revision.author, uuid, Some(&authors))?,
            committer: author_ident(&revision.author, uuid, Some(&authors))?,
            timestamp: timestamp.seconds,
            timezone_offset: timestamp.offset,
            message: commit_message(config, revision, uuid, &strip_prefix),
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
        &imported_revisions,
        None,
    )?;

    Ok(ImportSummary { imported_revisions })
}

fn import_ra_revisions_for_mapping(
    git: &GitCli,
    config: &SvnRemoteConfig,
    uuid: &str,
    mapping: &RefMapping,
    all_mappings: &[RefMapping],
    revisions: &[RevisionEvent],
    session: &impl RaSession,
) -> Result<ImportSummary, String> {
    let strip_prefix = strip_prefix_for(config, &mapping.svn_path);
    let mut stream = FastImportStream::new();
    let mut imported_revisions = Vec::new();
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
    let authors = author_mapper(config)?;
    let staging_ref = next_import_staging_ref();

    for revision in revisions {
        if revision.revision <= max_imported_revision {
            continue;
        }

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
            author: author_ident(&revision.author, uuid, Some(&authors))?,
            committer: author_ident(&revision.author, uuid, Some(&authors))?,
            timestamp: timestamp.seconds,
            timezone_offset: timestamp.offset,
            message: commit_message(config, revision, uuid, &strip_prefix),
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

        let filters = PathFilters::new(config.include_paths.clone(), config.ignore_paths.clone())?;
        let mut filtered_editor = FilteredFetchEditor {
            inner: &mut editor,
            filters: &filters,
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
        commit.changes = finalize_ra_changes(commit.changes, revision, &strip_prefix, config)?;
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
        &imported_revisions,
        append,
    )?;

    Ok(ImportSummary { imported_revisions })
}

struct FilteredFetchEditor<'a> {
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

fn finalize_ra_changes(
    changes: Vec<FileChange>,
    _revision: &RevisionEvent,
    strip_prefix: &str,
    config: &SvnRemoteConfig,
) -> Result<Vec<FileChange>, String> {
    let filters = PathFilters::new(config.include_paths.clone(), config.ignore_paths.clone())?;
    let mut filtered = Vec::new();
    for change in changes {
        let git_path = match &change {
            FileChange::Modify { path, .. } | FileChange::Delete { path } => path,
        };
        let svn_path = svn_path_for_git_path(strip_prefix, git_path);
        if path_is_included(&filters, &svn_path)? {
            filtered.push(change);
        }
    }

    Ok(filtered)
}

fn svn_path_for_git_path(strip_prefix: &str, git_path: &str) -> String {
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

struct CopyParentSource {
    commit: String,
    svn_path: String,
}

fn copy_parent_source(
    git: &GitCli,
    uuid: &str,
    mapping: &RefMapping,
    all_mappings: &[RefMapping],
    revision: &RevisionEvent,
) -> Result<Option<CopyParentSource>, String> {
    let mapping_path = mapping.svn_path.trim_matches('/');
    let mut copy_candidates = revision
        .changed_paths
        .iter()
        .filter(|changed_path| {
            let changed_path_text = changed_path.path.trim_matches('/');
            matches!(
                changed_path.action,
                ChangeAction::Add | ChangeAction::Replace
            ) && (changed_path_text == mapping_path
                || changed_path_text.starts_with(&format!("{mapping_path}/")))
        })
        .filter(|changed_path| {
            changed_path.copy_from_path.is_some() && changed_path.copy_from_rev.is_some()
        })
        .collect::<Vec<_>>();
    copy_candidates.sort_by_key(|changed_path| {
        (
            changed_path.path.trim_matches('/') != mapping_path,
            std::cmp::Reverse(changed_path.copy_from_rev),
        )
    });
    let Some(copy_candidate) = copy_candidates.first() else {
        return Ok(None);
    };
    let copy_source_path = copy_candidate
        .copy_from_path
        .as_deref()
        .expect("copy candidates require a source path")
        .trim_matches('/');
    let copy_source_revision = copy_candidate
        .copy_from_rev
        .expect("copy candidates require a source revision");

    let Some(source_mapping) = all_mappings.iter().find(|candidate| {
        let candidate_path = candidate.svn_path.trim_matches('/');
        copy_source_path == candidate_path
            || copy_source_path.starts_with(&format!("{candidate_path}/"))
    }) else {
        return Ok(None);
    };
    let path = rev_map_path(git, &source_mapping.git_ref, uuid)?;
    if !path.exists() {
        return Ok(None);
    };
    let rev_map = RevMap::open_existing(&path, git.object_format()?)?;
    let Some(commit) = resolve_copy_commit(
        &rev_map,
        copy_source_revision,
        revision.revision.saturating_sub(1),
    )?
    else {
        return Ok(None);
    };

    Ok(Some(CopyParentSource {
        commit,
        svn_path: source_mapping.svn_path.trim_matches('/').to_string(),
    }))
}

fn resolve_copy_commit(
    rev_map: &RevMap,
    declared_revision: u32,
    last_revision: u32,
) -> Result<Option<String>, String> {
    for revision in declared_revision..=last_revision.max(declared_revision) {
        if let Some(commit) = rev_map.get(revision)? {
            return Ok(Some(commit));
        }
    }
    Ok(rev_map
        .records()?
        .into_iter()
        .rev()
        .find(|record| {
            record.revision <= declared_revision
                && record.object_id_hex.bytes().any(|byte| byte != b'0')
        })
        .map(|record| record.object_id_hex))
}

fn prefixed_tree_entries(
    git: &GitCli,
    commit: &str,
    svn_path: &str,
) -> Result<Vec<TreeEntry>, String> {
    let svn_path = svn_path.trim_matches('/');
    let entries = git
        .tree_files(commit)?
        .into_iter()
        .map(|file| {
            let path = if svn_path.is_empty() {
                file.path
            } else {
                format!("{svn_path}/{}", file.path)
            };
            TreeEntry::file(path, file.mode, file.content)
        })
        .collect::<Vec<_>>();
    Ok(entries)
}

fn copy_from_parent_ref(
    git: &GitCli,
    uuid: &str,
    mapping: &RefMapping,
    all_mappings: &[RefMapping],
    revision: &RevisionEvent,
) -> Result<Option<String>, String> {
    let mapping_path = mapping.svn_path.trim_matches('/');
    let mut copy_candidates = revision
        .changed_paths
        .iter()
        .filter(|changed_path| {
            let changed_path_text = changed_path.path.trim_matches('/');
            matches!(
                changed_path.action,
                ChangeAction::Add | ChangeAction::Replace
            ) && changed_path.kind == NodeKind::Directory
                && (changed_path_text == mapping_path
                    || changed_path_text.starts_with(&format!("{mapping_path}/")))
                && changed_path.copy_from_path.is_some()
                && changed_path.copy_from_rev.is_some()
        })
        .collect::<Vec<_>>();
    copy_candidates.sort_by_key(|changed_path| std::cmp::Reverse(changed_path.copy_from_rev));

    for copy_candidate in copy_candidates {
        let source_path = copy_candidate
            .copy_from_path
            .as_deref()
            .unwrap_or_default()
            .trim_matches('/');
        let Some(source_revision) = copy_candidate.copy_from_rev else {
            continue;
        };
        let Some(source_mapping) = all_mappings.iter().find(|candidate| {
            let candidate_path = candidate.svn_path.trim_matches('/');
            source_path == candidate_path || source_path.starts_with(&format!("{candidate_path}/"))
        }) else {
            continue;
        };

        let path = rev_map_path(git, &source_mapping.git_ref, uuid)?;
        if !path.exists() {
            continue;
        }
        let rev_map = RevMap::open_existing(path, git.object_format()?)?;
        if let Some(commit) = resolve_copy_commit(&rev_map, source_revision, source_revision)? {
            return Ok(Some(commit));
        }
    }

    Ok(None)
}

fn max_imported_revision(git: &GitCli, refname: &str, uuid: &str) -> Result<u32, String> {
    let path = rev_map_path(git, refname, uuid)?;
    if !path.exists() {
        return Ok(0);
    }
    let rev_map = RevMap::open_existing(path, git.object_format()?)?;
    Ok(rev_map.max_revision(false)?.unwrap_or(0))
}

fn concrete_mappings(
    config: &SvnRemoteConfig,
    revisions: &[RevisionEvent],
) -> Result<Vec<RefMapping>, String> {
    let ignore_refs = compile_ref_filter(config.ignore_refs.as_deref())?;
    let mut mappings = Vec::<RefMapping>::new();
    for mapping in &config.fetch {
        let mut mapping = mapping.clone();
        mapping.git_ref = sanitize_refname(&mapping.git_ref)?;
        if let Some(existing) = mappings
            .iter_mut()
            .find(|candidate| candidate.svn_path == mapping.svn_path)
        {
            *existing = mapping;
        } else {
            mappings.push(mapping);
        }
    }
    for mapping in config.branches.iter().chain(config.tags.iter()) {
        let spec = GlobSpec::new(&mapping.svn_path, true)?;
        for wildcard in wildcard_matches(&spec, revisions) {
            let svn_path = spec.full_path(&wildcard);
            let git_ref = sanitize_refname(&mapping.git_ref.replace('*', &wildcard))?;
            if !ref_is_included(&git_ref, &ignore_refs)? {
                continue;
            }
            if !mappings
                .iter()
                .any(|candidate| candidate.svn_path == svn_path && candidate.git_ref == git_ref)
            {
                mappings.push(RefMapping {
                    kind: mapping.kind.clone(),
                    svn_path,
                    git_ref,
                });
            }
        }
    }
    Ok(mappings)
}

fn expand_ra_wildcard_ancestor_copies(
    session: &impl RaSession,
    config: &SvnRemoteConfig,
    revisions: &mut [RevisionEvent],
) -> Result<(), String> {
    let specs = config
        .branches
        .iter()
        .chain(config.tags.iter())
        .map(|mapping| GlobSpec::new(&mapping.svn_path, true))
        .collect::<Result<Vec<_>, _>>()?;

    for revision in revisions {
        let original_paths = revision.changed_paths.clone();
        for changed_path in original_paths {
            let (Some(copy_from_path), Some(copy_from_revision)) = (
                changed_path.copy_from_path.as_deref(),
                changed_path.copy_from_rev,
            ) else {
                continue;
            };
            if changed_path.kind != NodeKind::Directory
                || !matches!(
                    changed_path.action,
                    ChangeAction::Add | ChangeAction::Replace
                )
            {
                continue;
            }
            let destination_root = changed_path.path.trim_matches('/');
            for spec in &specs {
                if wildcard_for_path(spec, destination_root).is_some()
                    || !glob_can_descend_from(spec, destination_root)
                {
                    continue;
                }
                for wildcard in ra_wildcard_matches(session, spec, revision.revision)? {
                    let destination_path = spec.full_path(&wildcard);
                    if !mapping_contains_path(destination_root, &destination_path) {
                        continue;
                    }
                    let relative = destination_path
                        .strip_prefix(destination_root)
                        .unwrap_or_default()
                        .trim_matches('/');
                    let source_path = join_svn_path(copy_from_path, relative);
                    if let Some(existing) = revision
                        .changed_paths
                        .iter_mut()
                        .find(|candidate| candidate.path.trim_matches('/') == destination_path)
                    {
                        if existing.copy_from_path.is_none() {
                            existing.action = changed_path.action.clone();
                            existing.copy_from_path = Some(format!("/{source_path}"));
                            existing.copy_from_rev = Some(copy_from_revision);
                            existing.kind = NodeKind::Directory;
                        }
                        continue;
                    }
                    revision.changed_paths.push(ChangedPath {
                        path: format!("/{destination_path}"),
                        action: changed_path.action.clone(),
                        copy_from_path: Some(format!("/{source_path}")),
                        copy_from_rev: Some(copy_from_revision),
                        kind: NodeKind::Directory,
                        properties_modified: false,
                        content_modified: false,
                        properties: BTreeMap::new(),
                        content: None,
                    });
                }
            }
        }
    }
    Ok(())
}

fn glob_can_descend_from(spec: &GlobSpec, path: &str) -> bool {
    let path = path.trim_matches('/');
    let left = spec.left().trim_matches('/');
    path.is_empty()
        || left.is_empty()
        || path == left
        || left.starts_with(&format!("{path}/"))
        || path.starts_with(&format!("{left}/"))
}

fn ra_wildcard_matches(
    session: &impl RaSession,
    spec: &GlobSpec,
    revision: u32,
) -> Result<Vec<String>, String> {
    if session.check_path(spec.left(), revision)? != Some(crate::svn::ra::SvnNodeKind::Directory) {
        return Ok(Vec::new());
    }
    let mut matches = BTreeSet::new();
    collect_ra_wildcard_matches(session, spec, revision, spec.left(), "", &mut matches)?;
    Ok(matches.into_iter().collect())
}

fn collect_ra_wildcard_matches(
    session: &impl RaSession,
    spec: &GlobSpec,
    revision: u32,
    directory: &str,
    relative: &str,
    matches: &mut BTreeSet<String>,
) -> Result<(), String> {
    let listing = session.get_dir(directory, revision)?;
    for (name, entry) in listing.entries {
        let child_relative = join_svn_path(relative, &name);
        let components = child_relative
            .split('/')
            .filter(|component| !component.is_empty())
            .collect::<Vec<_>>();
        if components.len() >= spec.depth() {
            let wildcard = components[..spec.depth()].join("/");
            let full_path = spec.full_path(&wildcard);
            if spec.is_match(&full_path)
                && session.check_path(&full_path, revision)?
                    == Some(crate::svn::ra::SvnNodeKind::Directory)
            {
                matches.insert(wildcard);
            }
            continue;
        }
        if entry.kind == crate::svn::ra::SvnNodeKind::Directory {
            let child_directory = join_svn_path(directory, &name);
            collect_ra_wildcard_matches(
                session,
                spec,
                revision,
                &child_directory,
                &child_relative,
                matches,
            )?;
        }
    }
    Ok(())
}

fn join_svn_path(left: &str, right: &str) -> String {
    let left = left.trim_matches('/');
    let right = right.trim_matches('/');
    match (left.is_empty(), right.is_empty()) {
        (true, _) => right.to_string(),
        (_, true) => left.to_string(),
        (false, false) => format!("{left}/{right}"),
    }
}

fn ensure_auxiliary_copy_mappings(
    git: &GitCli,
    revisions: &[RevisionEvent],
    mappings: &mut Vec<RefMapping>,
) -> Result<BTreeMap<String, u32>, String> {
    let mut auxiliary = Vec::<(String, String)>::new();
    let mut start_revisions = BTreeMap::new();

    loop {
        let mapping_count = mappings.len();
        for revision in revisions {
            let mut copy_paths = revision
                .changed_paths
                .iter()
                .filter(|changed_path| {
                    changed_path.copy_from_path.is_some() && changed_path.copy_from_rev.is_some()
                })
                .collect::<Vec<_>>();
            copy_paths.sort_by_key(|changed_path| changed_path.path.trim_matches('/').len());
            let mut covered_destinations = Vec::<String>::new();
            for changed_path in copy_paths {
                let (Some(source_path), Some(source_revision)) = (
                    changed_path.copy_from_path.as_deref(),
                    changed_path.copy_from_rev,
                ) else {
                    continue;
                };
                let destination_path = changed_path.path.trim_matches('/');
                if covered_destinations
                    .iter()
                    .any(|root| mapping_contains_path(root, destination_path))
                {
                    continue;
                }
                let source_path = source_path.trim_matches('/');
                if mappings
                    .iter()
                    .any(|mapping| mapping.svn_path.trim_matches('/') == source_path)
                {
                    continue;
                }
                let Some(destination) = mappings
                    .iter()
                    .filter(|mapping| mapping_contains_path(&mapping.svn_path, destination_path))
                    .max_by_key(|mapping| mapping.svn_path.trim_matches('/').len())
                else {
                    continue;
                };
                if destination.svn_path.trim_matches('/') != destination_path {
                    continue;
                }
                covered_destinations.push(destination_path.to_string());

                if let Some((_, refname)) = auxiliary
                    .iter()
                    .find(|(path, _)| path.trim_matches('/') == source_path)
                    .cloned()
                {
                    mappings.push(RefMapping {
                        kind: MappingKind::Fetch,
                        svn_path: source_path.to_string(),
                        git_ref: refname.clone(),
                    });
                    start_revisions.insert(refname, source_revision);
                    continue;
                }

                let base = auxiliary_ref_base(&destination.git_ref);
                let mut refname = format!("{base}@{source_revision}");
                while mappings
                    .iter()
                    .any(|mapping| mapping.git_ref == refname && mapping.svn_path != source_path)
                    || auxiliary.iter().any(|(path, configured_ref)| {
                        configured_ref == &refname && path != source_path
                    })
                    || !existing_auxiliary_ref_matches(git, &refname, source_path)?
                {
                    refname.push('-');
                }
                auxiliary.push((source_path.to_string(), refname.clone()));
                start_revisions.insert(refname.clone(), source_revision);
                mappings.push(RefMapping {
                    kind: MappingKind::Fetch,
                    svn_path: source_path.to_string(),
                    git_ref: refname,
                });
            }
        }
        if mappings.len() == mapping_count {
            break;
        }
    }
    Ok(start_revisions)
}

fn existing_auxiliary_ref_matches(
    git: &GitCli,
    refname: &str,
    source_path: &str,
) -> Result<bool, String> {
    let Ok(commit) = git.rev_parse(refname) else {
        return Ok(true);
    };
    let message = git.commit_message(commit.trim())?;
    let Some(footer) = message
        .lines()
        .rev()
        .find(|line| line.starts_with("git-svn-id: "))
    else {
        return Ok(false);
    };
    let identity = crate::git_svn_id::GitSvnId::parse(footer)?;
    Ok(identity
        .url
        .trim_end_matches('/')
        .ends_with(&format!("/{}", source_path.trim_matches('/'))))
}

fn auxiliary_ref_base(refname: &str) -> &str {
    let Some(at) = refname.rfind('@') else {
        return refname;
    };
    let suffix = &refname[at + 1..];
    let digits = suffix.trim_end_matches('-');
    if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) {
        &refname[..at]
    } else {
        refname
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CopyDependency {
    destination_ref: String,
    source_ref: String,
    source_revision: u32,
}

fn copy_dependencies(mappings: &[RefMapping], revisions: &[RevisionEvent]) -> Vec<CopyDependency> {
    let mut dependencies = Vec::new();
    for revision in revisions {
        for changed_path in &revision.changed_paths {
            let (Some(source_path), Some(source_revision)) = (
                changed_path.copy_from_path.as_deref(),
                changed_path.copy_from_rev,
            ) else {
                continue;
            };
            let destination_path = changed_path.path.trim_matches('/');
            let source_path = source_path.trim_matches('/');
            let destination = mappings
                .iter()
                .find(|mapping| mapping_contains_path(&mapping.svn_path, destination_path));
            let source = mappings
                .iter()
                .filter(|mapping| mapping_contains_path(&mapping.svn_path, source_path))
                .max_by_key(|mapping| mapping.svn_path.trim_matches('/').len());
            let (Some(destination), Some(source)) = (destination, source) else {
                continue;
            };
            if destination.git_ref == source.git_ref {
                continue;
            }
            let dependency = CopyDependency {
                destination_ref: destination.git_ref.clone(),
                source_ref: source.git_ref.clone(),
                source_revision,
            };
            if !dependencies.contains(&dependency) {
                dependencies.push(dependency);
            }
        }
    }
    dependencies
}

fn mapping_contains_path(mapping_path: &str, path: &str) -> bool {
    let mapping_path = mapping_path.trim_matches('/');
    mapping_path.is_empty() || path == mapping_path || path.starts_with(&format!("{mapping_path}/"))
}

fn select_and_order_mappings<'a>(
    mappings: &'a [RefMapping],
    selected_ref: Option<&str>,
    revisions: &[RevisionEvent],
) -> Vec<&'a RefMapping> {
    let dependencies = copy_dependencies(mappings, revisions);
    let mut selected = match selected_ref {
        Some(refname) => BTreeSet::from([refname.to_string()]),
        None => mappings
            .iter()
            .map(|mapping| mapping.git_ref.clone())
            .collect(),
    };
    loop {
        let mut changed = false;
        for dependency in &dependencies {
            if selected.contains(&dependency.destination_ref)
                && selected.insert(dependency.source_ref.clone())
            {
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut remaining = mappings
        .iter()
        .filter(|mapping| selected.contains(&mapping.git_ref))
        .collect::<Vec<_>>();
    let mut ordered = Vec::new();
    while !remaining.is_empty() {
        let next = remaining
            .iter()
            .position(|mapping| {
                dependencies
                    .iter()
                    .filter(|dependency| dependency.destination_ref == mapping.git_ref)
                    .all(|dependency| {
                        !selected.contains(&dependency.source_ref)
                            || ordered.iter().any(|ordered: &&RefMapping| {
                                ordered.git_ref == dependency.source_ref
                            })
                    })
            })
            .unwrap_or(0);
        ordered.push(remaining.remove(next));
    }
    ordered
}

fn backfill_mock_copy_sources(
    backend: &impl SvnBackend,
    git: &GitCli,
    uuid: &str,
    revisions: &mut Vec<RevisionEvent>,
    mappings: &[RefMapping],
) -> Result<(), String> {
    let dependencies = copy_dependencies(mappings, revisions);
    for (source_ref, required_revision) in copy_source_requirements(&dependencies) {
        let Some(mapping) = mappings
            .iter()
            .find(|mapping| mapping.git_ref == source_ref)
        else {
            continue;
        };
        let imported = max_imported_revision(git, &source_ref, uuid)?;
        if imported >= required_revision {
            continue;
        }
        let history = backend.log(imported.saturating_add(1).max(1), required_revision)?;
        merge_revisions(revisions, history);
        if revisions_for_mapping(revisions, &mapping.svn_path).is_empty() {
            return Err(format!(
                "copy source {}@{required_revision} could not be backfilled",
                mapping.svn_path
            ));
        }
    }
    Ok(())
}

fn backfill_ra_copy_sources(
    session: &impl RaSession,
    git: &GitCli,
    uuid: &str,
    revisions: &mut Vec<RevisionEvent>,
    mappings: &[RefMapping],
) -> Result<(), String> {
    let dependencies = copy_dependencies(mappings, revisions);
    for (source_ref, required_revision) in copy_source_requirements(&dependencies) {
        let Some(mapping) = mappings
            .iter()
            .find(|mapping| mapping.git_ref == source_ref)
        else {
            continue;
        };
        let imported = max_imported_revision(git, &source_ref, uuid)?;
        if imported >= required_revision {
            continue;
        }
        let history = session.get_log(&[], imported.saturating_add(1).max(1), required_revision)?;
        merge_revisions(revisions, history);
        if revisions_for_mapping(revisions, &mapping.svn_path).is_empty() {
            return Err(format!(
                "copy source {}@{required_revision} could not be backfilled",
                mapping.svn_path
            ));
        }
    }
    Ok(())
}

fn copy_source_requirements(dependencies: &[CopyDependency]) -> BTreeMap<String, u32> {
    let mut requirements = BTreeMap::new();
    for dependency in dependencies {
        requirements
            .entry(dependency.source_ref.clone())
            .and_modify(|revision: &mut u32| {
                *revision = (*revision).max(dependency.source_revision)
            })
            .or_insert(dependency.source_revision);
    }
    requirements
}

fn merge_revisions(revisions: &mut Vec<RevisionEvent>, additional: Vec<RevisionEvent>) {
    let mut merged = revisions
        .drain(..)
        .map(|revision| (revision.revision, revision))
        .collect::<BTreeMap<_, _>>();
    for revision in additional {
        merged.entry(revision.revision).or_insert(revision);
    }
    *revisions = merged.into_values().collect();
}

fn compile_ref_filter(pattern: Option<&str>) -> Result<Option<Regex>, String> {
    pattern
        .map(Regex::new)
        .transpose()
        .map_err(|err| err.to_string())
}

fn ref_is_included(refname: &str, ignore_refs: &Option<Regex>) -> Result<bool, String> {
    match ignore_refs {
        Some(ignore_refs) => ignore_refs
            .is_match(refname)
            .map(|matches| !matches)
            .map_err(|err| err.to_string()),
        None => Ok(true),
    }
}

fn wildcard_matches(spec: &GlobSpec, revisions: &[RevisionEvent]) -> Vec<String> {
    let mut matches = Vec::new();
    for revision in revisions {
        for changed_path in &revision.changed_paths {
            for path in std::iter::once(changed_path.path.as_str())
                .chain(changed_path.copy_from_path.as_deref())
            {
                let path = path.trim_matches('/');
                if let Some(wildcard) = wildcard_for_path(spec, path)
                    && !matches.contains(&wildcard)
                {
                    matches.push(wildcard);
                }
            }
        }
    }
    matches
}

fn wildcard_for_path(spec: &GlobSpec, path: &str) -> Option<String> {
    if spec.is_match(path) {
        return wildcard_from_exact_match(spec, path);
    }

    let left = spec.left();
    let relative = if left.is_empty() {
        path
    } else {
        path.strip_prefix(&format!("{left}/"))?
    };
    let wildcard = relative
        .split('/')
        .take(spec.depth())
        .collect::<Vec<_>>()
        .join("/");
    if wildcard.is_empty() {
        return None;
    }
    let full_path = spec.full_path(&wildcard);
    (path == full_path || path.starts_with(&format!("{full_path}/"))).then_some(wildcard)
}

fn revisions_for_mapping(revisions: &[RevisionEvent], svn_path: &str) -> Vec<RevisionEvent> {
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

fn imports_initial_mapping_root(
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

fn wildcard_from_exact_match(spec: &GlobSpec, path: &str) -> Option<String> {
    let mut relative = path;
    if !spec.left().is_empty() {
        relative = relative.strip_prefix(spec.left())?.trim_start_matches('/');
    }
    if !spec.right().is_empty() {
        relative = relative.strip_suffix(spec.right())?.trim_end_matches('/');
    }
    (!relative.is_empty()).then(|| relative.to_string())
}

fn changes_for_revision(
    revision: &RevisionEvent,
    strip_prefix: &str,
    config: &SvnRemoteConfig,
) -> Result<Vec<FileChange>, String> {
    if revision.changed_paths.is_empty() {
        return Ok(mock_fixture_changes(revision.revision));
    }
    let filters = PathFilters::new(config.include_paths.clone(), config.ignore_paths.clone())?;

    let mut file_paths = Vec::new();
    for changed_path in &revision.changed_paths {
        if matches!(
            changed_path.action,
            ChangeAction::Add | ChangeAction::Modify | ChangeAction::Replace
        ) && changed_path.kind == NodeKind::File
            && path_is_included(&filters, &changed_path.path)?
            && let Some(path) = import_path(&changed_path.path, strip_prefix)
        {
            file_paths.push(path);
        }
    }

    let mut changes = Vec::new();
    for changed_path in &revision.changed_paths {
        if !path_is_included(&filters, &changed_path.path)? {
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
            if !path_is_included(&filters, &changed_path.path)? {
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

fn path_is_included(filters: &PathFilters, path: &str) -> Result<bool, String> {
    let path = path.trim_matches('/');
    Ok(filters.decide(path)? == FilterDecision::Include)
}

fn placeholder_path(path: &str, config: &SvnRemoteConfig) -> String {
    format!(
        "{}/{}",
        path.trim_end_matches('/'),
        config.placeholder_filename.trim_matches('/')
    )
}

fn mode_for_change(changed_path: &crate::svn::ChangedPath) -> String {
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

fn content_for_change(changed_path: &crate::svn::ChangedPath) -> Vec<u8> {
    let content = changed_path.content.clone().unwrap_or_default();
    if changed_path.properties.contains_key("svn:special") && content.starts_with(b"link ") {
        content[5..].to_vec()
    } else {
        content
    }
}

fn mock_fixture_changes(revision: u32) -> Vec<FileChange> {
    if revision < 2 {
        return Vec::new();
    }

    vec![FileChange::Modify {
        path: "src/lib.rs".to_string(),
        mode: "100644".to_string(),
        content: b"pub fn answer() -> u8 { 42 }\n".to_vec(),
    }]
}

fn import_path(path: &str, strip_prefix: &str) -> Option<String> {
    let path = path.trim_matches('/');
    let relative = if strip_prefix.is_empty() {
        path
    } else {
        path.strip_prefix(strip_prefix)?.trim_start_matches('/')
    };
    (!relative.is_empty()).then(|| relative.to_string())
}

fn commit_message(
    config: &SvnRemoteConfig,
    revision: &RevisionEvent,
    uuid: &str,
    strip_prefix: &str,
) -> String {
    if config.no_metadata {
        return revision.message.clone();
    }

    let root_url = config.rewrite_root.as_ref().unwrap_or(&config.url);
    let url = if strip_prefix.is_empty() {
        root_url.clone()
    } else {
        format!("{}/{}", root_url.trim_end_matches('/'), strip_prefix)
    };
    let footer = GitSvnId {
        url,
        revision: revision.revision,
        uuid: config
            .rewrite_uuid
            .clone()
            .unwrap_or_else(|| uuid.to_string()),
    }
    .to_footer();
    format!("{}\n\n{}\n", revision.message, footer)
}

struct AuthorMapper {
    file: Option<AuthorResolver>,
    prog: Option<String>,
}

fn author_mapper(config: &SvnRemoteConfig) -> Result<AuthorMapper, String> {
    let file = if let Some(path) = &config.authors_file {
        let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        Some(parse_authors_file(&contents)?)
    } else {
        None
    };
    Ok(AuthorMapper {
        file,
        prog: config.authors_prog.clone(),
    })
}

fn author_ident(author: &str, uuid: &str, mapper: Option<&AuthorMapper>) -> Result<String, String> {
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

fn run_authors_prog(program: &str, author: &str) -> Result<String, String> {
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

fn strip_prefix_for(config: &SvnRemoteConfig, svn_path: &str) -> String {
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

struct GitTimestamp {
    seconds: i64,
    offset: String,
}

fn svn_git_timestamp(value: &str, localtime: bool) -> Result<GitTimestamp, String> {
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

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod timestamp_tests {
    use super::{
        apply_placeholder_log, author_ident, commit_message, imports_initial_mapping_root,
        max_imported_revision, rev_map_path, svn_git_timestamp, validate_mapping_ref_collisions,
        validate_refname_namespace,
    };
    use crate::config::SvnRemoteConfig;
    use crate::git::GitCli;
    use crate::mapping::{MappingKind, RefMapping};
    use crate::svn::{ChangeAction, ChangedPath, NodeKind, RevisionEvent};
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn parses_svn_rfc3339_date_with_fractional_seconds() {
        let timestamp = svn_git_timestamp("2026-01-01T00:00:00.123456Z", false).unwrap();
        assert_eq!(timestamp.seconds, 1_767_225_600);
        assert_eq!(timestamp.offset, "+0000");
    }

    #[test]
    fn rejects_missing_or_invalid_svn_dates() {
        assert!(svn_git_timestamp("", false).is_err());
        assert!(svn_git_timestamp("not-a-date", false).is_err());
    }

    #[test]
    fn missing_rev_map_reports_zero_without_creating_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let git = GitCli::new(temp.path());
        git.init().unwrap();
        let path = rev_map_path(&git, "refs/remotes/git-svn", "uuid").unwrap();

        assert_eq!(
            max_imported_revision(&git, "refs/remotes/git-svn", "uuid").unwrap(),
            0
        );
        assert!(!path.exists());
        assert!(!path.parent().unwrap().exists());
    }

    #[test]
    fn placeholder_log_replays_only_events_visible_at_the_rev_map_tip() {
        let log = "\
r2
  +empty_dir: trunk/empty%20directory
r3
  -empty_dir: trunk/empty%20directory
r4
  +empty_dir: trunk/empty%20directory
";
        let mut at_r2 = BTreeSet::new();
        apply_placeholder_log(log, "trunk", 2, &mut at_r2).unwrap();
        assert_eq!(at_r2, ["empty directory".to_string()].into_iter().collect());

        let mut at_r3 = BTreeSet::new();
        apply_placeholder_log(log, "trunk", 3, &mut at_r3).unwrap();
        assert!(at_r3.is_empty());

        let mut at_r4 = BTreeSet::new();
        apply_placeholder_log(log, "trunk", 4, &mut at_r4).unwrap();
        assert_eq!(at_r4, ["empty directory".to_string()].into_iter().collect());
    }

    #[test]
    fn placeholder_log_ignores_other_mappings_and_rejects_bad_uri_encoding() {
        let mut ownership = BTreeSet::new();
        apply_placeholder_log(
            "r2\n  +empty_dir: branches/topic/empty\n",
            "trunk",
            2,
            &mut ownership,
        )
        .unwrap();
        assert!(ownership.is_empty());
        assert!(
            apply_placeholder_log(
                "r2\n  +empty_dir: trunk/bad%2\n",
                "trunk",
                2,
                &mut ownership,
            )
            .unwrap_err()
            .contains("invalid URI encoding")
        );
    }

    #[test]
    fn default_author_identity_uses_repository_uuid() {
        assert_eq!(
            author_ident("alice", "repo-uuid", None).unwrap(),
            "alice <alice@repo-uuid>"
        );
    }

    #[test]
    fn metadata_commit_message_has_the_frozen_trailing_newline() {
        let config = SvnRemoteConfig::new(
            "svn",
            "file:///repo".to_string(),
            crate::mapping::LayoutMappings {
                fetch: vec![crate::mapping::RefMapping {
                    kind: crate::mapping::MappingKind::Fetch,
                    svn_path: "trunk".to_string(),
                    git_ref: "refs/remotes/git-svn".to_string(),
                }],
                branches: Vec::new(),
                tags: Vec::new(),
            },
        );
        let revision = RevisionEvent {
            revision: 1,
            author: "alice".to_string(),
            message: "layout".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            changed_paths: Vec::new(),
        };

        assert_eq!(
            commit_message(&config, &revision, "repo-uuid", "trunk"),
            "layout\n\ngit-svn-id: file:///repo/trunk@1 repo-uuid\n"
        );
    }

    #[test]
    fn initial_mapping_root_addition_is_imported_as_an_empty_commit() {
        let revision = RevisionEvent {
            revision: 1,
            author: "alice".to_string(),
            message: "layout".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            changed_paths: vec![ChangedPath {
                path: "/trunk".to_string(),
                action: ChangeAction::Add,
                copy_from_path: None,
                copy_from_rev: None,
                kind: NodeKind::Directory,
                properties_modified: false,
                content_modified: false,
                properties: BTreeMap::new(),
                content: None,
            }],
        };

        assert!(imports_initial_mapping_root(&revision, "trunk", true));
        assert!(!imports_initial_mapping_root(&revision, "trunk", false));
        assert!(!imports_initial_mapping_root(
            &revision,
            "branches/main",
            true
        ));
    }

    #[test]
    fn rejects_git_ref_file_directory_collisions() {
        let refs = vec![
            "refs/remotes/origin/topic".to_string(),
            "refs/remotes/origin/topic/nested".to_string(),
        ];

        let error = validate_refname_namespace(&refs).unwrap_err();
        assert!(error.contains("cannot coexist"));
        assert!(error.contains("refs/remotes/origin/topic"));
        assert!(error.contains("refs/remotes/origin/topic/nested"));
    }

    #[test]
    fn rejects_distinct_svn_paths_mapped_to_the_same_ref() {
        let first = RefMapping {
            kind: MappingKind::Branches,
            svn_path: "branches/one".to_string(),
            git_ref: "refs/remotes/origin/topic".to_string(),
        };
        let second = RefMapping {
            kind: MappingKind::Tags,
            svn_path: "tags/one".to_string(),
            git_ref: first.git_ref.clone(),
        };

        let error = validate_mapping_ref_collisions(&[&first, &second]).unwrap_err();
        assert!(error.contains("maps both"));
        assert!(error.contains("branches/one"));
        assert!(error.contains("tags/one"));
    }
}

fn next_import_staging_ref() -> String {
    let sequence = IMPORT_STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("refs/git-svn-rs/import/{}-{sequence}", std::process::id())
}

fn validate_mapping_ref_collisions(mappings: &[&RefMapping]) -> Result<(), String> {
    let mut owners = BTreeMap::<&str, &str>::new();
    for mapping in mappings {
        if let Some(previous_path) =
            owners.insert(mapping.git_ref.as_str(), mapping.svn_path.as_str())
            && previous_path != mapping.svn_path
        {
            return Err(format!(
                "remote ref {} maps both SVN paths {} and {}; configure distinct destinations before fetching",
                mapping.git_ref, previous_path, mapping.svn_path
            ));
        }
    }
    Ok(())
}

fn validate_ref_storage_collisions(git: &GitCli, refnames: &[String]) -> Result<(), String> {
    let mut all_refnames = git.refs_under("refs")?;
    all_refnames.extend(refnames.iter().cloned());
    validate_refname_namespace(&all_refnames)
}

fn validate_refname_namespace(refnames: &[String]) -> Result<(), String> {
    let mut sorted = refnames.to_vec();
    sorted.sort();
    sorted.dedup();

    for (index, left) in sorted.iter().enumerate() {
        for right in &sorted[index + 1..] {
            if right.starts_with(&format!("{left}/")) {
                return Err(format!(
                    "remote refs {left} and {right} cannot coexist because one ref path contains the other"
                ));
            }
        }
    }

    Ok(())
}

fn scan_marker_refnames(
    config: &SvnRemoteConfig,
    selected_ref: Option<&str>,
) -> Result<BTreeSet<String>, String> {
    match selected_ref {
        Some(refname) => Ok(BTreeSet::from([refname.to_string()])),
        None => {
            let mut by_svn_path = BTreeMap::new();
            for mapping in &config.fetch {
                by_svn_path.insert(
                    mapping.svn_path.as_str(),
                    sanitize_refname(&mapping.git_ref)?,
                );
            }
            Ok(by_svn_path.into_values().collect())
        }
    }
}

fn publish_scan_marker(
    git: &GitCli,
    refname: &str,
    uuid: &str,
    scanned_end: u32,
) -> Result<(), String> {
    if max_imported_revision(git, refname, uuid)? >= scanned_end {
        return Ok(());
    }
    let object_format = git.object_format()?;
    let zero = "0".repeat(object_format.hex_len());
    let current_oid = git
        .rev_parse(refname)
        .ok()
        .map(|oid| oid.trim().to_string())
        .unwrap_or_else(|| zero.clone());
    complete_import_publication(
        git,
        ImportPublication {
            refname: refname.to_string(),
            expected_old_oid: current_oid.clone(),
            target_oid: current_oid,
            rev_map_path: rev_map_path(git, refname, uuid)?,
            records: vec![RevMapRecord {
                revision: scanned_end,
                object_id_hex: zero,
            }],
            append: None,
        },
    )
}

fn publish_imported_revisions(
    git: &GitCli,
    refname: &str,
    uuid: &str,
    expected_old_oid: Option<&str>,
    staging_ref: &str,
    revisions: &[u32],
    append: Option<ImportAppend>,
) -> Result<(), String> {
    let history = git.first_parent_history(staging_ref)?;
    if history.len() < revisions.len() {
        return Err("import staging ref does not contain every imported revision".to_string());
    }
    if let Some(expected_old_oid) = expected_old_oid
        && history.get(revisions.len()).map(String::as_str) != Some(expected_old_oid)
    {
        return Err(
            "import staging history does not descend from the expected ref tip".to_string(),
        );
    }
    let mut object_ids = history
        .into_iter()
        .take(revisions.len())
        .collect::<Vec<_>>();
    object_ids.reverse();
    let records = revisions
        .iter()
        .copied()
        .zip(object_ids)
        .map(|(revision, object_id_hex)| RevMapRecord {
            revision,
            object_id_hex,
        })
        .collect::<Vec<_>>();
    let target_oid = records
        .last()
        .ok_or_else(|| "import publication has no records".to_string())?
        .object_id_hex
        .clone();
    git.delete_ref_expected(staging_ref, &target_oid)?;
    let object_format = git.object_format()?;
    let expected_old_oid = expected_old_oid
        .map(str::to_string)
        .unwrap_or_else(|| "0".repeat(object_format.hex_len()));
    complete_import_publication(
        git,
        ImportPublication {
            refname: refname.to_string(),
            expected_old_oid,
            target_oid,
            rev_map_path: rev_map_path(git, refname, uuid)?,
            records,
            append,
        },
    )
}

fn rev_map_path(git: &GitCli, refname: &str, uuid: &str) -> Result<PathBuf, String> {
    let git_dir = git.git_dir()?;
    let git_dir = git.work_tree().join(git_dir);
    Ok(svn_metadata_dir(&git_dir, refname)?.join(format!(".rev_map.{uuid}")))
}

fn placeholder_ownership(
    git: &GitCli,
    mapping: &RefMapping,
    max_revision: u32,
) -> Result<BTreeSet<String>, String> {
    let metadata_dir = rev_map_path(git, &mapping.git_ref, "metadata")?
        .parent()
        .ok_or_else(|| "rev_map path has no parent directory".to_string())?
        .to_path_buf();
    let mut ownership = BTreeSet::new();
    let compressed = metadata_dir.join("unhandled.log.gz");
    if compressed.exists() {
        let file = std::fs::File::open(&compressed)
            .map_err(|error| format!("failed to read {}: {error}", compressed.display()))?;
        let mut contents = String::new();
        GzDecoder::new(file)
            .read_to_string(&mut contents)
            .map_err(|error| format!("failed to decompress {}: {error}", compressed.display()))?;
        apply_placeholder_log(&contents, &mapping.svn_path, max_revision, &mut ownership)?;
    }
    let current = metadata_dir.join("unhandled.log");
    if current.exists() {
        let contents = std::fs::read_to_string(&current)
            .map_err(|error| format!("failed to read {}: {error}", current.display()))?;
        apply_placeholder_log(&contents, &mapping.svn_path, max_revision, &mut ownership)?;
    }
    Ok(ownership)
}

fn apply_placeholder_log(
    contents: &str,
    svn_path: &str,
    max_revision: u32,
    ownership: &mut BTreeSet<String>,
) -> Result<(), String> {
    let mut revision = None;
    for line in contents.lines() {
        if let Some(value) = line.strip_prefix('r') {
            revision = value.parse::<u32>().ok();
            continue;
        }
        if !revision.is_some_and(|value| value <= max_revision) {
            continue;
        }
        let (present, encoded) = if let Some(path) = line.strip_prefix("  +empty_dir: ") {
            (true, path)
        } else if let Some(path) = line.strip_prefix("  -empty_dir: ") {
            (false, path)
        } else {
            continue;
        };
        let full_path = uri_decode(encoded)?;
        let full_path = full_path.trim_matches('/');
        let svn_path = svn_path.trim_matches('/');
        let relative = if svn_path.is_empty() {
            full_path
        } else if full_path == svn_path {
            ""
        } else if let Some(relative) = full_path.strip_prefix(&format!("{svn_path}/")) {
            relative
        } else {
            continue;
        };
        if relative.is_empty() {
            continue;
        }
        if present {
            ownership.insert(relative.to_string());
        } else {
            ownership.remove(relative);
        }
    }
    Ok(())
}

fn uri_decode(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        if index + 2 >= bytes.len() {
            return Err(format!("invalid URI encoding in unhandled.log: {value}"));
        }
        let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
            .map_err(|_| format!("invalid URI encoding in unhandled.log: {value}"))?;
        decoded.push(
            u8::from_str_radix(hex, 16)
                .map_err(|_| format!("invalid URI encoding in unhandled.log: {value}"))?,
        );
        index += 3;
    }
    String::from_utf8(decoded)
        .map_err(|_| format!("non-UTF-8 URI encoding in unhandled.log: {value}"))
}

fn unhandled_append(
    git: &GitCli,
    refname: &str,
    revisions: &[(u32, UnhandledMetadata)],
) -> Result<Option<ImportAppend>, String> {
    if revisions.is_empty() {
        return Ok(None);
    }

    let metadata_dir = rev_map_path(git, refname, "metadata")?
        .parent()
        .ok_or_else(|| "rev_map path has no parent directory".to_string())?
        .to_path_buf();
    let mut payload = String::new();
    for (revision, metadata) in revisions {
        payload.push_str(&format!("r{revision}\n"));
        for line in metadata.lines() {
            payload.push_str(&line);
            payload.push('\n');
        }
    }
    Ok(Some(ImportAppend {
        path: metadata_dir.join("unhandled.log"),
        payload: payload.into_bytes(),
    }))
}
