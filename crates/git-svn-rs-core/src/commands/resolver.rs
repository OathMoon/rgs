use std::path::{Path, PathBuf};

use crate::commands::reset_transaction;
use crate::config::{SvnRemoteConfig, read_svn_remote_config, svn_remote_names};
use crate::error::GitSvnError;
use crate::git::GitCli;
use crate::git_svn_id::GitSvnId;
use crate::mapping::{RefMapping, desanitize_refname, sanitize_refname};
use crate::metadata::svn_metadata_dir;
use crate::rev_map::{RevMap, RevMapRecord};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackedRevMap {
    pub uuid: String,
    pub path: PathBuf,
}

pub struct TrackedSvn {
    pub git: GitCli,
    pub config: SvnRemoteConfig,
    pub refname: String,
    pub svn_path: String,
    pub uuid: String,
    pub rev_map_path: PathBuf,
    pub source_rev_map: Option<TrackedRevMap>,
}

impl TrackedSvn {
    pub fn open_rev_map(&self) -> Result<RevMap, String> {
        self.open_rev_map_typed().map_err(|error| error.to_string())
    }

    pub fn open_rev_map_typed(&self) -> Result<RevMap, GitSvnError> {
        let format = self.git.object_format()?;
        RevMap::open_existing(&self.rev_map_path, format).map_err(GitSvnError::metadata_corruption)
    }

    pub fn records(&self) -> Result<Vec<RevMapRecord>, String> {
        self.records_typed().map_err(|error| error.to_string())
    }

    pub fn records_typed(&self) -> Result<Vec<RevMapRecord>, GitSvnError> {
        self.open_rev_map_typed()?
            .records()
            .map_err(GitSvnError::metadata_corruption)
    }

    pub fn max_record(&self) -> Result<Option<RevMapRecord>, String> {
        self.max_record_typed().map_err(|error| error.to_string())
    }

    pub fn max_record_typed(&self) -> Result<Option<RevMapRecord>, GitSvnError> {
        self.open_rev_map_typed()?
            .max_record(true)
            .map_err(GitSvnError::metadata_corruption)
    }
}

pub fn resolve_tracked_svn(work_tree: impl Into<PathBuf>) -> Result<TrackedSvn, String> {
    resolve_tracked_svn_typed(work_tree).map_err(|error| error.to_string())
}

pub fn resolve_tracked_svn_typed(work_tree: impl Into<PathBuf>) -> Result<TrackedSvn, GitSvnError> {
    resolve_tracked_svn_impl(work_tree.into(), "HEAD", false, false)
}

pub(crate) fn resolve_tracked_svn_allow_import_batch(
    work_tree: impl Into<PathBuf>,
) -> Result<TrackedSvn, String> {
    resolve_tracked_svn_allow_import_batch_typed(work_tree).map_err(|error| error.to_string())
}

pub(crate) fn resolve_tracked_svn_allow_import_batch_typed(
    work_tree: impl Into<PathBuf>,
) -> Result<TrackedSvn, GitSvnError> {
    resolve_tracked_svn_impl(work_tree.into(), "HEAD", false, true)
}

pub fn resolve_tracked_svn_at(
    work_tree: impl Into<PathBuf>,
    treeish: &str,
) -> Result<TrackedSvn, String> {
    resolve_tracked_svn_at_typed(work_tree, treeish).map_err(|error| error.to_string())
}

pub fn resolve_tracked_svn_at_typed(
    work_tree: impl Into<PathBuf>,
    treeish: &str,
) -> Result<TrackedSvn, GitSvnError> {
    resolve_tracked_svn_impl(work_tree.into(), treeish, true, false)
}

pub(crate) fn resolve_tracked_svn_path(
    tracked: &TrackedSvn,
    svn_path: &str,
) -> Result<TrackedSvn, String> {
    resolve_tracked_svn_path_typed(tracked, svn_path).map_err(|error| error.to_string())
}

pub(crate) fn resolve_tracked_svn_path_typed(
    tracked: &TrackedSvn,
    svn_path: &str,
) -> Result<TrackedSvn, GitSvnError> {
    let mut candidates = tracked_candidate_mappings(&tracked.git, &tracked.config)?
        .into_iter()
        .filter(|mapping| mapping.svn_path == svn_path)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.git_ref.cmp(&right.git_ref));
    candidates.dedup_by(|left, right| left.git_ref == right.git_ref);
    let mapping = match candidates.len() {
        0 => {
            return Err(GitSvnError::invalid_invocation(format!(
                "commit URL path {svn_path:?} does not match a tracked SVN mapping"
            )));
        }
        1 => candidates.pop().expect("one commit URL mapping"),
        _ => {
            return Err(GitSvnError::ambiguity(format!(
                "commit URL path {svn_path:?} matches multiple SVN mappings"
            )));
        }
    };
    let git_dir = tracked.git.git_dir()?;
    let metadata_root = tracked.git.work_tree().join(git_dir);
    let metadata_dir = svn_metadata_dir(&metadata_root, &mapping.git_ref)?;
    let resolved = tracked_from_mapping(&tracked.git, &tracked.config, &mapping, &metadata_dir)?;
    if resolved.uuid != tracked.uuid {
        return Err(GitSvnError::metadata_corruption(
            "commit URL mapping repository UUID does not match the tracked target",
        ));
    }
    Ok(resolved)
}

fn resolve_tracked_svn_impl(
    work_tree: PathBuf,
    treeish: &str,
    require_history_identity: bool,
    allow_import_batch: bool,
) -> Result<TrackedSvn, GitSvnError> {
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
        return Err(GitSvnError::metadata_corruption(
            "missing svn-remote.svn.url",
        ));
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
                    hard_errors.push((config.name.clone(), error.to_string()));
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
        return Err(GitSvnError::metadata_corruption(format_resolver_errors(
            "invalid SVN tracking metadata across remotes",
            hard_errors,
        )));
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
        return Err(GitSvnError::ambiguity(format!(
            "ambiguous SVN tracking identity at first-parent distance {distance}: {targets}"
        )));
    }

    if require_history_identity {
        return Err(GitSvnError::ambiguity(format!(
            "tree-ish {treeish:?} has no unambiguous SVN tracking identity"
        )));
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
        return Err(GitSvnError::ambiguity(format!(
            "ambiguous SVN tracking fallback across remotes: {remotes}"
        )));
    }

    if let Some((_, error)) = errors.iter().find(|(remote, _)| remote == "svn") {
        return Err(GitSvnError::metadata_corruption(error.clone()));
    }
    if errors.len() == 1 {
        return Err(GitSvnError::metadata_corruption(
            errors.pop().expect("one resolver error").1,
        ));
    }
    Err(GitSvnError::metadata_corruption(format!(
        "no usable SVN tracking metadata across remotes: {}",
        errors
            .into_iter()
            .map(|(remote, error)| format!("{remote}: {error}"))
            .collect::<Vec<_>>()
            .join("; ")
    )))
}

fn same_tracking_target(left: &TrackedSvn, right: &TrackedSvn) -> bool {
    left.config.name == right.config.name
        && left.refname == right.refname
        && left.svn_path == right.svn_path
        && left.uuid == right.uuid
        && left.rev_map_path == right.rev_map_path
        && left.source_rev_map == right.source_rev_map
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
    let selected = crate::tracking_state::select_existing_rev_maps(git, config, metadata_dir)?
        .ok_or_else(|| format!("missing .rev_map in {}", metadata_dir.display()))?;
    crate::tracking_state::validate_selected_rev_maps(git, config, &svn_path, &selected)?;
    let uuid = selected.transport_uuid;
    let rev_map_path = selected.transport_path;
    let source_rev_map = selected
        .source
        .map(|(uuid, path)| TrackedRevMap { uuid, path });

    Ok(TrackedSvn {
        git: git.clone(),
        config: config.clone(),
        refname,
        svn_path,
        uuid,
        rev_map_path,
        source_rev_map,
    })
}

fn rev_map_first_parent_identity(
    tracked: &TrackedSvn,
    first_parent_history: &[String],
) -> Result<Option<(usize, RevMapRecord)>, GitSvnError> {
    let records = tracked.records_typed()?;
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
    crate::tracking_state::tracking_identity_matches(
        &tracked.git,
        &tracked.config,
        &tracked.svn_path,
        crate::tracking_state::IdentityRevMaps {
            transport_uuid: &tracked.uuid,
            transport_record: record,
            source_path: tracked
                .source_rev_map
                .as_ref()
                .map(|source| source.path.as_path()),
        },
        Some(&identity),
    )
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

#[cfg(test)]
mod tests {
    use super::{TrackedSvn, resolve_tracked_svn_path_typed, tracked_from_mapping};
    use crate::config::SvnRemoteConfig;
    use crate::error::ErrorCategory;
    use crate::git::GitCli;
    use crate::mapping::{MappingKind, RefMapping, build_single_path};
    use crate::rev_map::{ObjectFormat, RevMap};

    #[test]
    fn resolves_configured_svm_dual_rev_maps_without_ambiguity() {
        const TRANSPORT: &str = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        const SOURCE: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let temp = tempfile::tempdir().unwrap();
        let git = GitCli::new(temp.path());
        git.init().unwrap();
        git.run_for_test(["config", "user.name", "Test"]).unwrap();
        git.run_for_test(["config", "user.email", "test@example.com"])
            .unwrap();
        git.run_for_test([
            "commit",
            "--allow-empty",
            "-m",
            &format!("import\n\ngit-svn-id: file:///source/trunk@105 {SOURCE}"),
        ])
        .unwrap();
        let oid = git.rev_parse("HEAD").unwrap().trim().to_string();
        git.git_svn_metadata_set("svn-remote.svn.uuid", TRANSPORT)
            .unwrap();
        let metadata_dir = temp.path().join(".git/svn/refs/remotes/git-svn");
        let transport_path = metadata_dir.join(format!(".rev_map.{TRANSPORT}"));
        let source_path = metadata_dir.join(format!(".rev_map.{SOURCE}"));
        RevMap::open(&transport_path, ObjectFormat::Sha1)
            .unwrap()
            .append(11, &oid)
            .unwrap();
        RevMap::open(&source_path, ObjectFormat::Sha1)
            .unwrap()
            .append(105, &oid)
            .unwrap();
        let config = SvnRemoteConfig::new("svn", "file:///repo", build_single_path("trunk"))
            .with_svm_identity("file:///source", "file:///repo", SOURCE);
        let mapping = RefMapping {
            kind: MappingKind::Fetch,
            svn_path: "trunk".to_string(),
            git_ref: "refs/remotes/git-svn".to_string(),
        };

        let tracked = tracked_from_mapping(&git, &config, &mapping, &metadata_dir).unwrap();
        assert_eq!(tracked.uuid, TRANSPORT);
        assert_eq!(tracked.rev_map_path, transport_path);
        assert_eq!(tracked.source_rev_map.unwrap().path, source_path);
    }

    #[test]
    fn typed_tracking_boundary_classifies_corrupt_rev_map() {
        let temp = tempfile::tempdir().unwrap();
        let git = GitCli::new(temp.path());
        git.init().unwrap();
        let rev_map_path = temp.path().join("corrupt.rev_map");
        std::fs::write(&rev_map_path, b"not-a-record").unwrap();
        let tracked = TrackedSvn {
            git,
            config: SvnRemoteConfig::new("svn", "file:///repo", build_single_path("trunk")),
            refname: "refs/remotes/git-svn".to_string(),
            svn_path: "trunk".to_string(),
            uuid: "uuid".to_string(),
            rev_map_path,
            source_rev_map: None,
        };

        let error = tracked.records_typed().unwrap_err();

        assert_eq!(error.category(), ErrorCategory::MetadataCorruption);
        assert!(error.to_string().starts_with("corrupt .rev_map"));
    }

    #[test]
    fn typed_tracking_boundary_classifies_mapping_ambiguity() {
        let temp = tempfile::tempdir().unwrap();
        let git = GitCli::new(temp.path());
        git.init().unwrap();
        let mut config = SvnRemoteConfig::new("svn", "file:///repo", build_single_path("trunk"));
        config.fetch = ["refs/remotes/one/trunk", "refs/remotes/two/trunk"]
            .into_iter()
            .map(|git_ref| RefMapping {
                kind: MappingKind::Fetch,
                svn_path: "trunk".to_string(),
                git_ref: git_ref.to_string(),
            })
            .collect();
        let tracked = TrackedSvn {
            git,
            config,
            refname: "refs/remotes/git-svn".to_string(),
            svn_path: "trunk".to_string(),
            uuid: "uuid".to_string(),
            rev_map_path: temp.path().join("unused.rev_map"),
            source_rev_map: None,
        };

        let error = resolve_tracked_svn_path_typed(&tracked, "trunk")
            .err()
            .expect("duplicate mappings should be ambiguous");

        assert_eq!(error.category(), ErrorCategory::Ambiguity, "{error}");
        assert_eq!(
            error.to_string(),
            "commit URL path \"trunk\" matches multiple SVN mappings"
        );
    }
}
