use super::*;

pub(super) struct CopyParentSource {
    pub(super) commit: String,
    pub(super) svn_path: String,
}

pub(super) fn copy_parent_source(
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

    let Some(source_mapping) = most_specific_mapping_for_path(all_mappings, copy_source_path)
    else {
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

pub(super) fn resolve_copy_commit(
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

pub(super) fn prefixed_tree_entries(
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

pub(super) fn copy_from_parent_ref(
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
        let Some(source_mapping) = most_specific_mapping_for_path(all_mappings, source_path) else {
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

pub(super) fn max_imported_revision(
    git: &GitCli,
    refname: &str,
    uuid: &str,
) -> Result<u32, String> {
    let path = rev_map_path(git, refname, uuid)?;
    if !path.exists() {
        return Ok(0);
    }
    let rev_map = RevMap::open_existing(path, git.object_format()?)?;
    Ok(rev_map.max_revision(false)?.unwrap_or(0))
}

pub(super) fn concrete_mappings(
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

pub(super) fn expand_ra_wildcard_ancestor_copies(
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

pub(super) fn glob_can_descend_from(spec: &GlobSpec, path: &str) -> bool {
    let path = path.trim_matches('/');
    let left = spec.left().trim_matches('/');
    path.is_empty()
        || left.is_empty()
        || path == left
        || left.starts_with(&format!("{path}/"))
        || path.starts_with(&format!("{left}/"))
}

pub(super) fn ra_wildcard_matches(
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

pub(super) fn collect_ra_wildcard_matches(
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

pub(super) fn join_svn_path(left: &str, right: &str) -> String {
    let left = left.trim_matches('/');
    let right = right.trim_matches('/');
    match (left.is_empty(), right.is_empty()) {
        (true, _) => right.to_string(),
        (_, true) => left.to_string(),
        (false, false) => format!("{left}/{right}"),
    }
}

pub(super) fn ensure_auxiliary_copy_mappings(
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

pub(super) fn existing_auxiliary_ref_matches(
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

pub(super) fn auxiliary_ref_base(refname: &str) -> &str {
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
pub(super) struct CopyDependency {
    destination_ref: String,
    source_ref: String,
    source_revision: u32,
}

pub(super) fn copy_dependencies(
    mappings: &[RefMapping],
    revisions: &[RevisionEvent],
) -> Vec<CopyDependency> {
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
            let destination = most_specific_mapping_for_path(mappings, destination_path);
            let source = most_specific_mapping_for_path(mappings, source_path);
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

pub(super) fn mapping_contains_path(mapping_path: &str, path: &str) -> bool {
    let mapping_path = mapping_path.trim_matches('/');
    mapping_path.is_empty() || path == mapping_path || path.starts_with(&format!("{mapping_path}/"))
}

pub(super) fn most_specific_mapping_for_path<'a>(
    mappings: &'a [RefMapping],
    path: &str,
) -> Option<&'a RefMapping> {
    mappings
        .iter()
        .filter(|mapping| mapping_contains_path(&mapping.svn_path, path))
        .max_by_key(|mapping| mapping.svn_path.trim_matches('/').len())
}

pub(super) fn select_and_order_mappings<'a>(
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

pub(super) fn backfill_mock_copy_sources(
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

pub(super) fn backfill_ra_copy_sources(
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

pub(super) fn copy_source_requirements(dependencies: &[CopyDependency]) -> BTreeMap<String, u32> {
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

pub(super) fn merge_revisions(revisions: &mut Vec<RevisionEvent>, additional: Vec<RevisionEvent>) {
    let mut merged = revisions
        .drain(..)
        .map(|revision| (revision.revision, revision))
        .collect::<BTreeMap<_, _>>();
    for revision in additional {
        merged.entry(revision.revision).or_insert(revision);
    }
    *revisions = merged.into_values().collect();
}

pub(super) fn compile_ref_filter(pattern: Option<&str>) -> Result<Option<Regex>, String> {
    pattern
        .map(Regex::new)
        .transpose()
        .map_err(|err| err.to_string())
}

pub(super) fn ref_is_included(refname: &str, ignore_refs: &Option<Regex>) -> Result<bool, String> {
    match ignore_refs {
        Some(ignore_refs) => ignore_refs
            .is_match(refname)
            .map(|matches| !matches)
            .map_err(|err| err.to_string()),
        None => Ok(true),
    }
}

pub(super) fn wildcard_matches(spec: &GlobSpec, revisions: &[RevisionEvent]) -> Vec<String> {
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

pub(super) fn wildcard_for_path(spec: &GlobSpec, path: &str) -> Option<String> {
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
