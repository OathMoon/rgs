use super::*;

pub(super) fn validate_existing_tracking_states(
    git: &GitCli,
    config: &SvnRemoteConfig,
    selected_ref: Option<&str>,
) -> Result<(), String> {
    let mappings = crate::commands::resolver::tracked_candidate_mappings(git, config)?
        .into_iter()
        .filter(|mapping| selected_ref.is_none_or(|selected_ref| mapping.git_ref == selected_ref))
        .collect();
    crate::tracking_state::validate_candidate_mappings(git, config, mappings)?;
    Ok(())
}

pub(super) fn validate_requested_urls_before_recovery(
    git: &GitCli,
    args: &FetchArgs,
) -> Result<(), String> {
    if args.parent {
        let tracked =
            crate::commands::resolver::resolve_tracked_svn_allow_import_batch(git.work_tree())?;
        return crate::path_url::validate_fetch_url(&tracked.config.url);
    }
    let remotes = if args.fetch_all {
        svn_remote_names(git)?
    } else {
        vec![args.remote.clone().unwrap_or_else(|| "svn".to_string())]
    };
    let git_dir = std::path::PathBuf::from(git.git_dir()?);
    let git_dir = if git_dir.is_absolute() {
        git_dir
    } else {
        git.work_tree().join(git_dir)
    };
    let fresh_metadata = !git_dir.join("svn").exists();
    for remote in remotes {
        let config = read_svn_remote_config(git, &remote)?;
        crate::path_url::validate_fetch_url(&config.url)?;
        if fresh_metadata
            && matches!(
                svn_url_profile(&config.url),
                SvnUrlProfile::Http | SvnUrlProfile::Https
            )
        {
            configured_backend(&config, &args.shared)?.repository_root()?;
        }
    }
    Ok(())
}

pub(super) fn verify_remote_fetch_ref_sanity(git: &GitCli) -> Result<(), String> {
    let mut keys = git.config_names_matching(r"^svn-remote\..*\.(fetch|branches|tags)$")?;
    keys.sort();
    keys.dedup();
    let mut destinations = Vec::<ConfiguredDestination>::new();
    for key in keys {
        let kind = if key.ends_with(".branches") {
            MappingKind::Branches
        } else if key.ends_with(".tags") {
            MappingKind::Tags
        } else {
            MappingKind::Fetch
        };
        for value in git.config_get_all(&key)? {
            let mapping = parse_mapping(&value, kind.clone())?;
            let owner = format!("{key}={value}");
            let remote = key
                .strip_prefix("svn-remote.")
                .and_then(|key| key.rsplit_once('.').map(|(remote, _)| remote))
                .ok_or_else(|| format!("invalid SVN remote mapping key: {key}"))?;
            let destination = ConfiguredDestination::new(remote.to_string(), mapping, owner)?;
            if let Some(previous) = destinations
                .iter()
                .find(|previous| previous.may_overlap(&destination))
            {
                let description = if previous.pattern == destination.pattern {
                    format!("remote ref {}", destination.pattern)
                } else {
                    format!(
                        "remote ref destinations {} and {}",
                        previous.pattern, destination.pattern
                    )
                };
                return Err(format!(
                    "{description} may be tracked by both {} and {}; resolve this ambiguity before fetching",
                    previous.owner, destination.owner
                ));
            }
            destinations.push(destination);
        }
    }
    Ok(())
}

#[derive(Debug)]
pub(super) struct ConfiguredDestination {
    remote: String,
    owner: String,
    pattern: String,
    wildcard_depth: Option<usize>,
}

impl ConfiguredDestination {
    fn new(remote: String, mapping: RefMapping, owner: String) -> Result<Self, String> {
        let wildcard_depth = match mapping.kind {
            MappingKind::Fetch => None,
            MappingKind::Branches | MappingKind::Tags if mapping.git_ref.contains('*') => {
                Some(GlobSpec::new(&mapping.svn_path, true)?.depth())
            }
            MappingKind::Branches | MappingKind::Tags => None,
        };
        let mut pattern = crate::mapping::sanitize_refname(&mapping.git_ref)?;
        if wildcard_depth.is_some() {
            pattern = pattern.replace("%2A", "*");
        }
        Ok(Self {
            remote,
            owner,
            pattern,
            wildcard_depth,
        })
    }

    fn may_overlap(&self, other: &Self) -> bool {
        if self.remote == other.remote {
            return self.pattern == other.pattern;
        }
        if self.wildcard_depth.is_none() && other.wildcard_depth.is_none() {
            return self.pattern == other.pattern;
        }
        if self.expanded_slash_count() != other.expanded_slash_count() {
            return false;
        }

        let (self_prefix, self_suffix) = literal_edges(&self.pattern);
        let (other_prefix, other_suffix) = literal_edges(&other.pattern);
        (self_prefix.starts_with(other_prefix) || other_prefix.starts_with(self_prefix))
            && (self_suffix.ends_with(other_suffix) || other_suffix.ends_with(self_suffix))
    }

    fn expanded_slash_count(&self) -> usize {
        let literal_slashes = self.pattern.bytes().filter(|byte| *byte == b'/').count();
        let wildcard_count = self.pattern.bytes().filter(|byte| *byte == b'*').count();
        literal_slashes + wildcard_count * self.wildcard_depth.unwrap_or(1).saturating_sub(1)
    }
}

pub(super) fn literal_edges(pattern: &str) -> (&str, &str) {
    let prefix = pattern
        .split_once('*')
        .map_or(pattern, |(prefix, _)| prefix);
    let suffix = pattern
        .rsplit_once('*')
        .map_or(pattern, |(_, suffix)| suffix);
    (prefix, suffix)
}

pub(super) fn imported_base_revision(
    git: &GitCli,
    config: &SvnRemoteConfig,
    uuid: &str,
    selected_ref: Option<&str>,
) -> Result<u32, String> {
    let git_dir = git.git_dir()?;
    let svn_dir = git.work_tree().join(git_dir).join("svn");
    if let Some(selected_ref) = selected_ref {
        return imported_ref_revision(git, &svn_dir, selected_ref, uuid);
    }

    let mut bases = Vec::new();
    for mapping in &config.fetch {
        bases.push(imported_ref_revision(
            git,
            &svn_dir,
            &mapping.git_ref,
            uuid,
        )?);
    }
    if !config.branches.is_empty() {
        bases.push(discovery_high_water(git, config, "branches")?.unwrap_or(0));
    }
    if !config.tags.is_empty() {
        bases.push(discovery_high_water(git, config, "tags")?.unwrap_or(0));
    }
    Ok(bases.into_iter().min().unwrap_or(0))
}

pub(super) fn imported_ref_revision(
    git: &GitCli,
    svn_dir: &std::path::Path,
    refname: &str,
    uuid: &str,
) -> Result<u32, String> {
    let object_format = git.object_format()?;
    let git_dir = svn_dir.parent().ok_or_else(|| {
        format!(
            "SVN metadata path has no Git directory: {}",
            svn_dir.display()
        )
    })?;
    let path =
        crate::metadata::svn_metadata_dir(git_dir, refname)?.join(format!(".rev_map.{uuid}"));
    if !path.exists() {
        return Ok(0);
    }
    Ok(RevMap::open_existing(path, object_format)?
        .max_revision(false)?
        .unwrap_or(0))
}
