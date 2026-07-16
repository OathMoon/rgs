use std::path::{Path, PathBuf};

use crate::config::SvnRemoteConfig;
use crate::git::GitCli;
use crate::git_svn_id::GitSvnId;
use crate::mapping::{MappingKind, RefMapping};
use crate::metadata::svn_metadata_dir;
use crate::path_url::add_path_to_url;
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
        RevMap::open(&self.rev_map_path, self.git.object_format()?)
    }

    pub fn records(&self) -> Result<Vec<RevMapRecord>, String> {
        self.open_rev_map()?.records()
    }

    pub fn max_record(&self) -> Result<Option<RevMapRecord>, String> {
        self.open_rev_map()?.max_record(true)
    }
}

pub fn resolve_tracked_svn(work_tree: impl Into<PathBuf>) -> Result<TrackedSvn, String> {
    resolve_tracked_svn_impl(work_tree.into(), "HEAD", false)
}

pub fn resolve_tracked_svn_at(
    work_tree: impl Into<PathBuf>,
    treeish: &str,
) -> Result<TrackedSvn, String> {
    resolve_tracked_svn_impl(work_tree.into(), treeish, true)
}

fn resolve_tracked_svn_impl(
    work_tree: PathBuf,
    treeish: &str,
    require_history_identity: bool,
) -> Result<TrackedSvn, String> {
    let git = GitCli::new(work_tree);
    let config = read_remote_config(&git, "svn")?;
    let first_mapping = config
        .fetch
        .first()
        .ok_or_else(|| "svn remote has no fetch mapping".to_string())?;
    let mappings = tracked_candidate_mappings(&git, &config)?;
    let git_dir = git.git_dir()?;
    let git_metadata_dir = git.work_tree().join(git_dir);
    let first_parent_history = git.first_parent_history(treeish)?;
    let mut fallback = None;
    let mut best_identity: Option<(usize, TrackedSvn)> = None;

    for mapping in &mappings {
        let metadata_dir =
            svn_metadata_dir(&git_metadata_dir, &short_ref_for_metadata(&mapping.git_ref));
        let tracked = match tracked_from_mapping(&git, &config, mapping, &metadata_dir) {
            Ok(tracked) => tracked,
            Err(_) if mapping != first_mapping => continue,
            Err(error) => return Err(error),
        };
        if let Some((distance, record)) =
            rev_map_first_parent_identity(&tracked, &first_parent_history)?
        {
            if !tracking_identity_matches(&tracked, &record, &first_parent_history[distance])? {
                continue;
            }
            if best_identity
                .as_ref()
                .is_none_or(|(best_distance, _)| distance < *best_distance)
            {
                best_identity = Some((distance, tracked));
            } else if best_identity
                .as_ref()
                .is_some_and(|(best_distance, _)| distance == *best_distance)
            {
                return Err(format!(
                    "ambiguous SVN tracking identity at first-parent distance {distance}"
                ));
            }
            continue;
        }
        if fallback.is_none() {
            fallback = Some(tracked);
        }
    }

    if let Some((_, tracked)) = best_identity {
        return Ok(tracked);
    }

    if require_history_identity {
        return Err(format!(
            "tree-ish {treeish:?} has no unambiguous SVN tracking identity"
        ));
    }

    if let Some(tracked) = fallback {
        return Ok(tracked);
    }

    let metadata_dir = svn_metadata_dir(
        &git_metadata_dir,
        &short_ref_for_metadata(&first_mapping.git_ref),
    );
    tracked_from_mapping(&git, &config, first_mapping, &metadata_dir)
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
    let rev_map = RevMap::open(&tracked.rev_map_path, tracked.git.object_format()?)?;
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
    let root = tracked
        .config
        .rewrite_root
        .as_ref()
        .unwrap_or(&tracked.config.url);
    let expected_url = add_path_to_url(root, &tracked.svn_path);
    let expected_uuid = tracked
        .config
        .rewrite_uuid
        .as_deref()
        .unwrap_or(&tracked.uuid);
    Ok(identity.url == expected_url
        && identity.uuid == expected_uuid
        && identity.revision == record.revision)
}

fn read_remote_config(git: &GitCli, remote: &str) -> Result<SvnRemoteConfig, String> {
    let prefix = format!("svn-remote.{remote}");
    let url = git
        .config_get(&format!("{prefix}.url"))?
        .ok_or_else(|| format!("missing {prefix}.url"))?;
    let fetch = git.config_get_all(&format!("{prefix}.fetch"))?;
    let branches = git.config_get_all(&format!("{prefix}.branches"))?;
    let tags = git.config_get_all(&format!("{prefix}.tags"))?;
    let mappings = fetch
        .into_iter()
        .map(|value| parse_mapping(&value, MappingKind::Fetch))
        .collect::<Result<Vec<_>, _>>()?;
    let branch_mappings = branches
        .into_iter()
        .map(|value| parse_mapping(&value, MappingKind::Branches))
        .collect::<Result<Vec<_>, _>>()?;
    let tag_mappings = tags
        .into_iter()
        .map(|value| parse_mapping(&value, MappingKind::Tags))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(SvnRemoteConfig {
        name: remote.to_string(),
        url,
        fetch: mappings,
        branches: branch_mappings,
        tags: tag_mappings,
        ignore_paths: git.config_get(&format!("{prefix}.ignore-paths"))?,
        include_paths: git.config_get(&format!("{prefix}.include-paths"))?,
        ignore_refs: git.config_get(&format!("{prefix}.ignore-refs"))?,
        authors_file: git.config_get(&format!("{prefix}.authors-file"))?,
        authors_prog: git.config_get(&format!("{prefix}.authors-prog"))?,
        log_window_size: git
            .config_get(&format!("{prefix}.log-window-size"))?
            .map(|value| {
                value
                    .parse()
                    .map_err(|_| format!("invalid {prefix}.log-window-size: {value}"))
            })
            .transpose()?,
        localtime: git
            .config_get(&format!("{prefix}.localtime"))?
            .is_some_and(|value| value == "true"),
        username: git.config_get(&format!("{prefix}.username"))?,
        config_dir: git.config_get(&format!("{prefix}.config-dir"))?,
        no_auth_cache: git
            .config_get(&format!("{prefix}.no-auth-cache"))?
            .is_some_and(|value| value == "true"),
        no_metadata: git
            .config_get(&format!("{prefix}.noMetadata"))?
            .is_some_and(|value| value == "true"),
        rewrite_root: git.config_get(&format!("{prefix}.rewriteRoot"))?,
        rewrite_uuid: git.config_get(&format!("{prefix}.rewriteUUID"))?,
        preserve_empty_dirs: git
            .config_get(&format!("{prefix}.preserve-empty-dirs"))?
            .is_some_and(|value| value == "true"),
        placeholder_filename: git
            .config_get(&format!("{prefix}.placeholder-filename"))?
            .unwrap_or_else(|| ".gitignore".to_string()),
    })
}

fn parse_mapping(value: &str, kind: MappingKind) -> Result<RefMapping, String> {
    let (svn_path, git_ref) = value
        .split_once(':')
        .ok_or_else(|| format!("invalid fetch mapping: {value}"))?;
    Ok(RefMapping {
        kind,
        svn_path: svn_path.trim_start_matches('+').to_string(),
        git_ref: git_ref.to_string(),
    })
}

fn tracked_candidate_mappings(
    git: &GitCli,
    config: &SvnRemoteConfig,
) -> Result<Vec<RefMapping>, String> {
    let mut mappings = config.fetch.clone();
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
            let wildcard = refname.strip_prefix(git_prefix)?.strip_suffix(git_suffix)?;
            Some(RefMapping {
                kind: mapping.kind.clone(),
                svn_path: format!("{svn_prefix}{wildcard}{svn_suffix}"),
                git_ref: refname.clone(),
            })
        })
        .collect()
}

fn find_rev_map(metadata_dir: &Path) -> Result<(String, PathBuf), String> {
    let entries = std::fs::read_dir(metadata_dir).map_err(|e| e.to_string())?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if let Some(uuid) = name.strip_prefix(".rev_map.")
            && !uuid.ends_with(".lock")
        {
            return Ok((uuid.to_string(), path));
        }
    }
    Err(format!("missing .rev_map in {}", metadata_dir.display()))
}

fn short_ref_for_metadata(refname: &str) -> String {
    refname
        .strip_prefix("refs/remotes/")
        .unwrap_or(refname)
        .replace('/', ".")
}
