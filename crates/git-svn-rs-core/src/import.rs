use std::cell::OnceCell;
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
    ImportAppend, ImportPublication, ImportRevMapUpdate, begin_or_resume_batch,
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
mod discovery;
mod publication;
mod replay;

use discovery::*;
use publication::*;
use replay::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportOptions {
    pub start_revision: u32,
    pub end_revision: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    pub imported_revisions: Vec<u32>,
}

#[derive(Default)]
struct ImportRuntime {
    authors: OnceCell<Result<AuthorMapper, String>>,
    filters: OnceCell<Result<PathFilters, String>>,
}

struct MappingImportContext<'a> {
    git: &'a GitCli,
    config: &'a SvnRemoteConfig,
    uuid: &'a str,
    all_mappings: &'a [RefMapping],
    runtime: &'a ImportRuntime,
}

impl ImportRuntime {
    fn authors<'a>(&'a self, config: &SvnRemoteConfig) -> Result<&'a AuthorMapper, String> {
        self.authors
            .get_or_init(|| author_mapper(config))
            .as_ref()
            .map_err(Clone::clone)
    }

    fn filters<'a>(&'a self, config: &SvnRemoteConfig) -> Result<&'a PathFilters, String> {
        self.filters
            .get_or_init(|| {
                PathFilters::new(config.include_paths.clone(), config.ignore_paths.clone())
            })
            .as_ref()
            .map_err(Clone::clone)
    }
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
    let runtime = ImportRuntime::default();
    import_mock_revisions_for_ref_with_runtime(
        backend,
        git,
        config,
        options,
        selected_ref,
        &runtime,
    )
}

fn import_mock_revisions_for_ref_with_runtime(
    backend: &impl SvnBackend,
    git: &GitCli,
    config: &SvnRemoteConfig,
    options: ImportOptions,
    selected_ref: Option<&str>,
    runtime: &ImportRuntime,
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
            runtime,
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

    let import_context = MappingImportContext {
        git,
        config,
        uuid: &uuid,
        all_mappings: &mappings,
        runtime,
    };
    let mut completed_refs = BTreeSet::new();
    for mapping in selected_mappings {
        let mut mapping_revisions = revisions_for_mapping(&revisions, &mapping.svn_path);
        if let Some(start) = auxiliary_revisions.get(&mapping.git_ref) {
            mapping_revisions.retain(|revision| revision.revision >= *start);
        }
        if !mapping_revisions.is_empty() {
            let summary =
                import_revisions_for_mapping(&import_context, mapping, &mapping_revisions)?;
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
    let runtime = ImportRuntime::default();
    import_ra_revisions_for_ref_with_runtime(session, git, config, options, selected_ref, &runtime)
}

fn import_ra_revisions_for_ref_with_runtime(
    session: &impl RaSession,
    git: &GitCli,
    config: &SvnRemoteConfig,
    options: ImportOptions,
    selected_ref: Option<&str>,
    runtime: &ImportRuntime,
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
            runtime,
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
    let import_context = MappingImportContext {
        git,
        config,
        uuid: &uuid,
        all_mappings: &mappings,
        runtime,
    };
    let mut completed_refs = BTreeSet::new();
    for mapping in selected_mappings {
        let mut mapping_revisions = revisions_for_mapping(&revisions, &mapping.svn_path);
        if let Some(start) = auxiliary_revisions.get(&mapping.git_ref) {
            mapping_revisions.retain(|revision| revision.revision >= *start);
        }
        if !mapping_revisions.is_empty() {
            let summary = import_ra_revisions_for_mapping(
                &import_context,
                mapping,
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
    runtime: &ImportRuntime,
) -> Result<ImportSummary, String> {
    let options = ImportOptions {
        end_revision: Some(options.end_revision.unwrap_or(backend.latest_revnum()?)),
        ..options
    };
    import_in_windows(config, options, window_size, |window_config, options| {
        import_mock_revisions_for_ref_with_runtime(
            backend,
            git,
            window_config,
            options,
            selected_ref,
            runtime,
        )
    })
}

fn import_ra_revisions_in_windows(
    session: &impl RaSession,
    git: &GitCli,
    config: &SvnRemoteConfig,
    options: ImportOptions,
    selected_ref: Option<&str>,
    window_size: u32,
    runtime: &ImportRuntime,
) -> Result<ImportSummary, String> {
    let options = ImportOptions {
        end_revision: Some(options.end_revision.unwrap_or(session.latest_revnum()?)),
        ..options
    };
    import_in_windows(config, options, window_size, |window_config, options| {
        import_ra_revisions_for_ref_with_runtime(
            session,
            git,
            window_config,
            options,
            selected_ref,
            runtime,
        )
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

#[cfg(test)]
mod tests;
