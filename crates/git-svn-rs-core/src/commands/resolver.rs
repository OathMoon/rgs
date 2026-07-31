use std::path::{Path, PathBuf};

use crate::commands::reset_transaction;
use crate::config::{SvnRemoteConfig, read_svn_remote_config, svn_remote_names};
use crate::git::GitCli;
use crate::git_svn_id::GitSvnId;
use crate::mapping::{RefMapping, desanitize_refname, sanitize_refname};
use crate::metadata::svn_metadata_dir;
use crate::rev_map::{RevMap, RevMapRecord};

pub struct TrackedSvn {
    pub git: GitCli,
    pub config: SvnRemoteConfig,
    pub refname: String,
    pub svn_path: String,
    pub uuid: String,
    pub rev_map_path: PathBuf,
}

impl TrackedSvn {
    pub fn open_rev_map(&self) -> Result<RevMap, String> {
        RevMap::open_existing(&self.rev_map_path, self.git.object_format()?)
    }

    pub fn records(&self) -> Result<Vec<RevMapRecord>, String> {
        self.open_rev_map()?.records()
    }

    pub fn max_record(&self) -> Result<Option<RevMapRecord>, String> {
        self.open_rev_map()?.max_record(true)
    }
}

pub fn resolve_tracked_svn(work_tree: impl Into<PathBuf>) -> Result<TrackedSvn, String> {
    resolve_tracked_svn_impl(work_tree.into(), "HEAD", false, false)
}

pub(crate) fn resolve_tracked_svn_allow_import_batch(
    work_tree: impl Into<PathBuf>,
) -> Result<TrackedSvn, String> {
    resolve_tracked_svn_impl(work_tree.into(), "HEAD", false, true)
}

pub fn resolve_tracked_svn_at(
    work_tree: impl Into<PathBuf>,
    treeish: &str,
) -> Result<TrackedSvn, String> {
    resolve_tracked_svn_impl(work_tree.into(), treeish, true, false)
}

pub(crate) fn resolve_tracked_svn_path(
    tracked: &TrackedSvn,
    svn_path: &str,
) -> Result<TrackedSvn, String> {
    let mut candidates = tracked_candidate_mappings(&tracked.git, &tracked.config)?
        .into_iter()
        .filter(|mapping| mapping.svn_path == svn_path)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.git_ref.cmp(&right.git_ref));
    candidates.dedup_by(|left, right| left.git_ref == right.git_ref);
    let mapping = match candidates.len() {
        0 => {
            return Err(format!(
                "commit URL path {svn_path:?} does not match a tracked SVN mapping"
            ));
        }
        1 => candidates.pop().expect("one commit URL mapping"),
        _ => {
            return Err(format!(
                "commit URL path {svn_path:?} matches multiple SVN mappings"
            ));
        }
    };
    let git_dir = tracked.git.git_dir()?;
    let metadata_root = tracked.git.work_tree().join(git_dir);
    let metadata_dir = svn_metadata_dir(&metadata_root, &mapping.git_ref)?;
    let resolved = tracked_from_mapping(&tracked.git, &tracked.config, &mapping, &metadata_dir)?;
    if resolved.uuid != tracked.uuid {
        return Err("commit URL mapping repository UUID does not match the tracked target".into());
    }
    Ok(resolved)
}

fn resolve_tracked_svn_impl(
    work_tree: PathBuf,
    treeish: &str,
    require_history_identity: bool,
    allow_import_batch: bool,
) -> Result<TrackedSvn, String> {
    crate::migration::ensure_supported_git_svn_metadata(&work_tree)?;
    let git = GitCli::new(work_tree);
    if allow_import_batch {
        crate::import_transaction::ensure_no_publication_pending(&git)?;
    } else {
        crate::import_transaction::ensure_no_pending(&git)?;
    }
    reset_transaction::ensure_no_pending(&git)?;
    let remote_names = svn_remote_names(&git)?;
    if remote_names.is_empty() {
        return Err("missing svn-remote.svn.url".to_string());
    }
    let configs = remote_names
        .iter()
        .map(|remote| read_svn_remote_config(&git, remote))
        .collect::<Result<Vec<_>, _>>()?;
    let git_dir = git.git_dir()?;
    let git_metadata_dir = git.work_tree().join(git_dir);
    let first_parent_history = git.first_parent_history(treeish)?;
    let mut best_distance = None;
    let mut best_identities = Vec::new();
    let mut fallbacks = Vec::new();
    let mut errors = Vec::new();
    let mut hard_errors = Vec::new();

    for config in &configs {
        let mappings = tracked_candidate_mappings(&git, config)?;
        if mappings.is_empty() {
            errors.push((
                config.name.clone(),
                format!("SVN remote {} has no candidate mapping", config.name),
            ));
            continue;
        }
        let mut remote_fallback = None;
        let mut remote_error = None;
        for mapping in &mappings {
            let metadata_dir = svn_metadata_dir(&git_metadata_dir, &mapping.git_ref)?;
            let tracked = match tracked_from_mapping(&git, config, mapping, &metadata_dir) {
                Ok(tracked) => tracked,
                Err(error) => {
                    if is_missing_tracking_metadata_error(&error) {
                        remote_error.get_or_insert(error);
                    } else {
                        hard_errors.push((config.name.clone(), error));
                    }
                    continue;
                }
            };
            let identity = match rev_map_first_parent_identity(&tracked, &first_parent_history) {
                Ok(identity) => identity,
                Err(error) => {
                    hard_errors.push((config.name.clone(), error));
                    continue;
                }
            };
            if let Some((distance, record)) = identity
                && tracking_identity_matches(&tracked, &record, &first_parent_history[distance])?
            {
                match best_distance {
                    None => {
                        best_distance = Some(distance);
                        best_identities.push(tracked);
                    }
                    Some(current) if distance < current => {
                        best_distance = Some(distance);
                        best_identities.clear();
                        best_identities.push(tracked);
                    }
                    Some(current) if distance == current => {
                        if !best_identities
                            .iter()
                            .any(|candidate| same_tracking_target(candidate, &tracked))
                        {
                            best_identities.push(tracked);
                        }
                    }
                    Some(_) => {}
                }
                continue;
            }
            remote_fallback.get_or_insert(tracked);
        }
        if let Some(tracked) = remote_fallback {
            fallbacks.push(tracked);
        }
        if let Some(error) = remote_error {
            errors.push((config.name.clone(), error));
        }
    }

    if !hard_errors.is_empty() {
        return Err(format_resolver_errors(
            "invalid SVN tracking metadata across remotes",
            hard_errors,
        ));
    }

    if best_identities.len() == 1 {
        return Ok(best_identities.pop().expect("one best tracking identity"));
    }
    if best_identities.len() > 1 {
        let distance = best_distance.expect("ambiguous identities have a distance");
        let targets = best_identities
            .iter()
            .map(|tracked| format!("{}/{}", tracked.config.name, tracked.refname))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "ambiguous SVN tracking identity at first-parent distance {distance}: {targets}"
        ));
    }

    if require_history_identity {
        return Err(format!(
            "tree-ish {treeish:?} has no unambiguous SVN tracking identity"
        ));
    }

    if let Some(index) = fallbacks
        .iter()
        .position(|tracked| tracked.config.name == "svn")
    {
        return Ok(fallbacks.swap_remove(index));
    }
    if fallbacks.len() == 1 {
        return Ok(fallbacks.pop().expect("one fallback tracking identity"));
    }
    if fallbacks.len() > 1 {
        let remotes = fallbacks
            .iter()
            .map(|tracked| tracked.config.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "ambiguous SVN tracking fallback across remotes: {remotes}"
        ));
    }

    if let Some((_, error)) = errors.iter().find(|(remote, _)| remote == "svn") {
        return Err(error.clone());
    }
    if errors.len() == 1 {
        return Err(errors.pop().expect("one resolver error").1);
    }
    Err(format!(
        "no usable SVN tracking metadata across remotes: {}",
        errors
            .into_iter()
            .map(|(remote, error)| format!("{remote}: {error}"))
            .collect::<Vec<_>>()
            .join("; ")
    ))
}

fn same_tracking_target(left: &TrackedSvn, right: &TrackedSvn) -> bool {
    left.config.name == right.config.name
        && left.refname == right.refname
        && left.svn_path == right.svn_path
        && left.uuid == right.uuid
        && left.rev_map_path == right.rev_map_path
}

fn is_missing_tracking_metadata_error(error: &str) -> bool {
    error.starts_with("missing .rev_map in ")
        || error.starts_with("missing SVN metadata directory ")
}

fn format_resolver_errors(prefix: &str, errors: Vec<(String, String)>) -> String {
    format!(
        "{prefix}: {}",
        errors
            .into_iter()
            .map(|(remote, error)| format!("{remote}: {error}"))
            .collect::<Vec<_>>()
            .join("; ")
    )
}

fn tracked_from_mapping(
    git: &GitCli,
    config: &SvnRemoteConfig,
    mapping: &RefMapping,
    metadata_dir: &Path,
) -> Result<TrackedSvn, String> {
    let refname = mapping.git_ref.clone();
    let svn_path = mapping.svn_path.clone();
    let (uuid, rev_map_path) = find_rev_map(metadata_dir)?;

    Ok(TrackedSvn {
        git: git.clone(),
        config: config.clone(),
        refname,
        svn_path,
        uuid,
        rev_map_path,
    })
}

fn rev_map_first_parent_identity(
    tracked: &TrackedSvn,
    first_parent_history: &[String],
) -> Result<Option<(usize, RevMapRecord)>, String> {
    let rev_map = RevMap::open_existing(&tracked.rev_map_path, tracked.git.object_format()?)?;
    let records = rev_map.records()?;
    Ok(first_parent_history
        .iter()
        .enumerate()
        .find_map(|(distance, commit)| {
            records
                .iter()
                .find(|record| record.object_id_hex == *commit)
                .cloned()
                .map(|record| (distance, record))
        }))
}

fn tracking_identity_matches(
    tracked: &TrackedSvn,
    record: &RevMapRecord,
    commit: &str,
) -> Result<bool, String> {
    if tracked.config.no_metadata {
        return Ok(true);
    }
    let message = tracked.git.commit_message(commit)?;
    let Some(footer) = message.lines().rev().find(|line| !line.trim().is_empty()) else {
        return Ok(false);
    };
    let Ok(identity) = GitSvnId::parse(footer.trim_end_matches('\r')) else {
        return Ok(false);
    };
    let expected_url = tracked.config.metadata_url(&tracked.svn_path)?;
    let expected_uuid = tracked.config.metadata_uuid(&tracked.uuid)?;
    Ok(identity.url == expected_url
        && identity.uuid == expected_uuid
        && identity.revision == record.revision)
}

pub(crate) fn tracked_candidate_mappings(
    git: &GitCli,
    config: &SvnRemoteConfig,
) -> Result<Vec<RefMapping>, String> {
    let mut mappings = config
        .fetch
        .iter()
        .cloned()
        .map(|mut mapping| {
            mapping.git_ref = sanitize_refname(&mapping.git_ref)?;
            Ok(mapping)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let refs = git.refs_under("refs/remotes")?;
    for mapping in config.branches.iter().chain(config.tags.iter()) {
        mappings.extend(expand_ref_mapping(mapping, &refs));
    }
    Ok(mappings)
}

fn expand_ref_mapping(mapping: &RefMapping, refs: &[String]) -> Vec<RefMapping> {
    let Some((git_prefix, git_suffix)) = mapping.git_ref.split_once('*') else {
        return vec![mapping.clone()];
    };
    let Some((svn_prefix, svn_suffix)) = mapping.svn_path.split_once('*') else {
        return Vec::new();
    };

    refs.iter()
        .filter(|refname| !refname.ends_with("/HEAD"))
        .filter_map(|refname| {
            let raw_refname = desanitize_refname(refname);
            let wildcard = raw_refname
                .strip_prefix(git_prefix)?
                .strip_suffix(git_suffix)?;
            Some(RefMapping {
                kind: mapping.kind.clone(),
                svn_path: format!("{svn_prefix}{wildcard}{svn_suffix}"),
                git_ref: refname.clone(),
            })
        })
        .collect()
}

fn find_rev_map(metadata_dir: &Path) -> Result<(String, PathBuf), String> {
    let entries = std::fs::read_dir(metadata_dir).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!("missing SVN metadata directory {}", metadata_dir.display())
        } else {
            error.to_string()
        }
    })?;
    let mut candidates = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if let Some(uuid) = name.strip_prefix(".rev_map.")
            && !uuid.ends_with(".lock")
            && path.is_file()
        {
            candidates.push((uuid.to_string(), path));
        }
    }
    candidates.sort_by(|left, right| left.1.cmp(&right.1));
    match candidates.len() {
        0 => Err(format!("missing .rev_map in {}", metadata_dir.display())),
        1 => Ok(candidates.pop().expect("one rev_map candidate")),
        _ => Err(format!(
            "ambiguous .rev_map files in {}: {}",
            metadata_dir.display(),
            candidates
                .iter()
                .map(|(_, path)| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}
