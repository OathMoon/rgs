use super::*;

pub(super) struct ResolvedDcommitTarget {
    pub(super) repository_root: String,
    pub(super) commit_url: String,
    pub(super) svn_path: String,
    pub(super) mapping_ref: String,
    pub(super) mapping_svn_path: String,
    pub(super) rev_map_path: PathBuf,
    pub(super) commit_url_override: bool,
}

pub(super) fn resolve_dcommit_target(
    tracked: &crate::commands::resolver::TrackedSvn,
    commit_url: Option<&str>,
) -> Result<ResolvedDcommitTarget, String> {
    if let Some(commit_url) = commit_url {
        if tracked.config.url.starts_with("mock://") {
            return Err(
                "--commit-url is not supported for mock:// dcommit write-back in v1".to_string(),
            );
        }
        return resolve_full_commit_url_target(tracked, commit_url);
    }

    if let Some(commit_url) = &tracked.config.commit_url {
        if tracked.config.url.starts_with("mock://") {
            return Err(
                "svn-remote.<name>.commiturl is not supported for mock:// dcommit write-back in v1"
                    .to_string(),
            );
        }
        return resolve_full_commit_url_target(tracked, commit_url);
    }

    if tracked.config.push_url.is_some() && tracked.config.url.starts_with("mock://") {
        return Err(
            "svn-remote.<name>.pushurl is not supported for mock:// dcommit write-back in v1"
                .to_string(),
        );
    }
    let repository_root = tracked
        .config
        .push_url
        .as_ref()
        .unwrap_or(&tracked.config.url)
        .clone();
    Ok(ResolvedDcommitTarget {
        commit_url: svn_checkout_url(&repository_root, &tracked.svn_path),
        repository_root,
        svn_path: tracked.svn_path.clone(),
        mapping_ref: tracked.refname.clone(),
        mapping_svn_path: tracked.svn_path.clone(),
        rev_map_path: tracked.rev_map_path.clone(),
        commit_url_override: false,
    })
}

pub(super) fn resolve_full_commit_url_target(
    tracked: &crate::commands::resolver::TrackedSvn,
    commit_url: &str,
) -> Result<ResolvedDcommitTarget, String> {
    let svn_path = commit_url_path(&tracked.config.url, commit_url)?;
    let mapping = resolve_tracked_svn_path(tracked, &svn_path)?;
    Ok(ResolvedDcommitTarget {
        repository_root: commit_url.to_string(),
        commit_url: commit_url.to_string(),
        svn_path: String::new(),
        mapping_ref: mapping.refname,
        mapping_svn_path: mapping.svn_path,
        rev_map_path: mapping.rev_map_path,
        commit_url_override: true,
    })
}

pub(super) fn is_svn_cli_write_back_url(url: &str) -> bool {
    matches!(
        crate::path_url::svn_url_profile(url),
        crate::path_url::SvnUrlProfile::File
            | crate::path_url::SvnUrlProfile::Svn
            | crate::path_url::SvnUrlProfile::Http
            | crate::path_url::SvnUrlProfile::Https
            | crate::path_url::SvnUrlProfile::SvnSsh
    )
}

pub(super) fn commit_url_path(remote_url: &str, commit_url: &str) -> Result<String, String> {
    let remote_url = crate::path_url::canonicalize_url(remote_url);
    let commit_url = crate::path_url::canonicalize_url(commit_url);
    if commit_url == remote_url {
        return Ok(String::new());
    }
    commit_url
        .strip_prefix(&format!("{remote_url}/"))
        .map(str::to_owned)
        .ok_or_else(|| {
            format!(
                "commit URL {commit_url} is outside the configured SVN remote {remote_url}; refusing before write setup"
            )
        })
}

pub(super) fn svn_checkout_url(root_url: &str, svn_path: &str) -> String {
    if svn_path.is_empty() {
        root_url.trim_end_matches('/').to_string()
    } else {
        format!(
            "{}/{}",
            root_url.trim_end_matches('/'),
            svn_path.trim_matches('/')
        )
    }
}
