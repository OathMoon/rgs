use std::path::{Path, PathBuf};

use crate::config::SvnRemoteConfig;
use crate::git::GitCli;
use crate::mapping::{MappingKind, RefMapping};
use crate::metadata::svn_metadata_dir;
use crate::rev_map::{ObjectFormat, RevMap, RevMapRecord};

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
        RevMap::open(&self.rev_map_path, ObjectFormat::Sha1)
    }

    pub fn records(&self) -> Result<Vec<RevMapRecord>, String> {
        self.open_rev_map()?.records()
    }

    pub fn max_record(&self) -> Result<Option<RevMapRecord>, String> {
        self.open_rev_map()?.max_record(true)
    }
}

pub fn resolve_tracked_svn(work_tree: impl Into<PathBuf>) -> Result<TrackedSvn, String> {
    let git = GitCli::new(work_tree.into());
    let config = read_remote_config(&git, "svn")?;
    let first_mapping = config
        .fetch
        .first()
        .ok_or_else(|| "svn remote has no fetch mapping".to_string())?;
    let git_dir = git.git_dir()?;
    let git_metadata_dir = git.work_tree().join(git_dir);
    let head = git
        .rev_parse("HEAD")
        .ok()
        .map(|value| value.trim().to_string());
    let mut fallback = None;
    let mut best_ancestor: Option<(u32, TrackedSvn)> = None;

    for mapping in &config.fetch {
        let metadata_dir =
            svn_metadata_dir(&git_metadata_dir, &short_ref_for_metadata(&mapping.git_ref));
        let tracked = match tracked_from_mapping(&git, &config, mapping, &metadata_dir) {
            Ok(tracked) => tracked,
            Err(_) if mapping != first_mapping => continue,
            Err(error) => return Err(error),
        };
        if let Some(head) = &head
            && let Some(score) = rev_map_head_score(&tracked, head)?
        {
            if score == u32::MAX {
                return Ok(tracked);
            }
            if best_ancestor
                .as_ref()
                .is_none_or(|(best_score, _)| score > *best_score)
            {
                best_ancestor = Some((score, tracked));
            }
            continue;
        }
        if fallback.is_none() {
            fallback = Some(tracked);
        }
    }

    if let Some((_, tracked)) = best_ancestor {
        return Ok(tracked);
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

fn rev_map_head_score(tracked: &TrackedSvn, head: &str) -> Result<Option<u32>, String> {
    let rev_map = RevMap::open(&tracked.rev_map_path, ObjectFormat::Sha1)?;
    let records = rev_map.records()?;
    if records.iter().any(|record| record.object_id_hex == head) {
        return Ok(Some(u32::MAX));
    }
    if tracked.git.is_ancestor(&tracked.refname, head)? {
        return Ok(records.iter().map(|record| record.revision).max());
    }
    Ok(None)
}

fn read_remote_config(git: &GitCli, remote: &str) -> Result<SvnRemoteConfig, String> {
    let prefix = format!("svn-remote.{remote}");
    let url = git
        .config_get(&format!("{prefix}.url"))?
        .ok_or_else(|| format!("missing {prefix}.url"))?;
    let fetch = git.config_get_all(&format!("{prefix}.fetch"))?;
    let mappings = fetch
        .into_iter()
        .map(|value| parse_fetch_mapping(&value))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(SvnRemoteConfig {
        name: remote.to_string(),
        url,
        fetch: mappings,
        branches: Vec::new(),
        tags: Vec::new(),
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

fn parse_fetch_mapping(value: &str) -> Result<RefMapping, String> {
    let (svn_path, git_ref) = value
        .split_once(':')
        .ok_or_else(|| format!("invalid fetch mapping: {value}"))?;
    Ok(RefMapping {
        kind: MappingKind::Fetch,
        svn_path: svn_path.to_string(),
        git_ref: git_ref.to_string(),
    })
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
