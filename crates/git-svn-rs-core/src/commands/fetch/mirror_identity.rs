use super::*;

pub(super) fn discovery_high_water(
    git: &GitCli,
    config: &SvnRemoteConfig,
    kind: &str,
) -> Result<Option<u32>, String> {
    let key = format!("svn-remote.{}.{kind}-maxRev", config.name);
    git.git_svn_metadata_get(&key)?
        .map(|value| {
            value
                .parse::<u32>()
                .map_err(|_| format!("invalid git-svn discovery high-water {key}={value}"))
        })
        .transpose()
}

pub(super) fn persist_discovery_high_water(
    git: &GitCli,
    config: &SvnRemoteConfig,
    selected_ref: Option<&str>,
    scanned_end: u32,
) -> Result<(), String> {
    if selected_ref.is_some() {
        return Ok(());
    }
    for (kind, configured) in [
        ("branches", !config.branches.is_empty()),
        ("tags", !config.tags.is_empty()),
    ] {
        if !configured {
            continue;
        }
        let current = discovery_high_water(git, config, kind)?.unwrap_or(0);
        if scanned_end > current {
            let key = format!("svn-remote.{}.{kind}-maxRev", config.name);
            git.git_svn_metadata_set(&key, &scanned_end.to_string())?;
        }
    }
    Ok(())
}

pub(super) fn hydrate_svm_identity(
    git: &GitCli,
    config: &mut SvnRemoteConfig,
    selected_ref: Option<&str>,
    repository_root: &str,
    latest_revision: u32,
    mut read_directory_properties: impl FnMut(
        &str,
        u32,
    ) -> Result<
        std::collections::BTreeMap<String, Vec<u8>>,
        String,
    >,
) -> Result<(), String> {
    if !config.use_svm_props {
        return Ok(());
    }
    config.validate_metadata_options()?;
    config.validate_svm_cache()?;
    if config.svm_source.is_some() {
        return Ok(());
    }

    let paths = svm_discovery_paths(config, selected_ref, repository_root)?;
    for path in &paths {
        let properties = read_directory_properties(path, latest_revision)?;
        let source = properties
            .get("svm:source")
            .filter(|value| !value.is_empty());
        let uuid = properties.get("svm:uuid").filter(|value| !value.is_empty());
        let (Some(source), Some(uuid)) = (source, uuid) else {
            continue;
        };
        let source = normalize_svm_source(source)?;
        let uuid = decode_svm_property("svm:uuid", uuid)?;
        crate::config::validate_svm_uuid(&uuid)?;
        let replace = crate::path_url::add_path_to_url(repository_root, path);

        let prefix = format!("svn-remote.{}", config.name);
        let source_key = format!("{prefix}.svm-source");
        let replace_key = format!("{prefix}.svm-replace");
        let uuid_key = format!("{prefix}.svm-uuid");
        git.git_svn_metadata_set_many(&[
            (&source_key, &source),
            (&replace_key, &replace),
            (&uuid_key, &uuid),
        ])?;
        config.svm_source = Some(source);
        config.svm_replace = Some(replace);
        config.svm_uuid = Some(uuid);
        return Ok(());
    }

    let tried = paths
        .iter()
        .map(|path| {
            format!(
                "  {}",
                crate::path_url::add_path_to_url(repository_root, path)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Err(format!(
        "useSvmProps set, but failed to read SVM properties\n(svm:source, svm:uuid) from the following URLs:\n{tried}\n"
    ))
}

pub(super) fn svm_discovery_paths(
    config: &SvnRemoteConfig,
    selected_ref: Option<&str>,
    repository_root: &str,
) -> Result<Vec<String>, String> {
    let session_path = crate::path_url::repository_relative_url_path(repository_root, &config.url)?;
    let mappings = config
        .fetch
        .iter()
        .chain(config.branches.iter())
        .chain(config.tags.iter())
        .filter(|mapping| selected_ref.is_none_or(|selected| mapping.git_ref == selected))
        .collect::<Vec<_>>();
    let starts = if mappings.is_empty() {
        vec![session_path.clone()]
    } else {
        mappings
            .into_iter()
            .map(|mapping| {
                let fixed = mapping
                    .svn_path
                    .split('/')
                    .take_while(|part| !part.contains('*') && !part.contains('{'))
                    .collect::<Vec<_>>()
                    .join("/");
                crate::path_url::join_paths([session_path.as_str(), fixed.as_str()])
            })
            .collect()
    };

    let mut paths = Vec::new();
    for start in starts {
        let mut current = start.trim_matches('/').to_string();
        loop {
            if !paths.contains(&current) {
                paths.push(current.clone());
            }
            let Some((parent, _)) = current.rsplit_once('/') else {
                if !current.is_empty() && !paths.iter().any(String::is_empty) {
                    paths.push(String::new());
                }
                break;
            };
            current = parent.to_string();
        }
    }
    if paths.is_empty() {
        paths.push(String::new());
    }
    Ok(paths)
}

pub(super) fn decode_svm_property(name: &str, value: &[u8]) -> Result<String, String> {
    let mut value = String::from_utf8(value.to_vec())
        .map_err(|_| format!("{name} directory property is not valid UTF-8"))?;
    if value.ends_with('\n') {
        value.pop();
    }
    Ok(value)
}

pub(super) fn normalize_svm_source(value: &[u8]) -> Result<String, String> {
    let mut source = decode_svm_property("svm:source", value)?;
    if let Some(bang) = source.find('!') {
        let left = source[..bang].trim_end_matches('/');
        let right = source[bang + 1..].trim_start_matches('/');
        source = format!("{left}/{right}");
    }
    while source.ends_with('/') {
        source.pop();
    }
    if let Some((scheme, rest)) = source.split_once("://")
        && scheme
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'+')
    {
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        if let Some((_, host)) = authority.rsplit_once('@') {
            source = if path.is_empty() {
                format!("{scheme}://{host}")
            } else {
                format!("{scheme}://{host}/{path}")
            };
        }
    }
    if source.is_empty() {
        return Err("svm:source directory property is empty after normalization".to_string());
    }
    Ok(source)
}

pub(super) fn hydrate_svnsync_identity(
    git: &GitCli,
    config: &mut SvnRemoteConfig,
    read_revision_zero: impl FnOnce() -> Result<std::collections::BTreeMap<String, Vec<u8>>, String>,
) -> Result<(), String> {
    if !config.use_svnsync_props {
        return Ok(());
    }
    config.validate_metadata_options()?;
    config.validate_svnsync_cache()?;
    if config.svnsync_url.is_some() {
        return Ok(());
    }

    let properties = read_revision_zero()?;
    let read = |name: &str| -> Result<String, String> {
        let value = properties
            .get(name)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!("useSvnsyncProps set, but failed to read svnsync property: {name}")
            })?;
        String::from_utf8(value.clone())
            .map_err(|_| format!("{name} revision property is not valid UTF-8"))
    };
    let url = read("svn:sync-from-url")?;
    let uuid = read("svn:sync-from-uuid")?;
    crate::config::validate_svnsync_identity(&url, &uuid)?;

    let prefix = format!("svn-remote.{}", config.name);
    let uuid_key = format!("{prefix}.svnsync-uuid");
    let url_key = format!("{prefix}.svnsync-url");
    git.git_svn_metadata_set_many(&[(&uuid_key, &uuid), (&url_key, &url)])?;
    config.svnsync_url = Some(url);
    config.svnsync_uuid = Some(uuid);
    Ok(())
}

pub(super) fn persist_repository_identity(
    git: &GitCli,
    config: &SvnRemoteConfig,
    repos_root: &str,
    uuid: &str,
) -> Result<(), String> {
    let prefix = format!("svn-remote.{}", config.name);
    git.git_svn_metadata_set(&format!("{prefix}.reposRoot"), repos_root)?;
    git.git_svn_metadata_set(&format!("{prefix}.uuid"), uuid)
}
