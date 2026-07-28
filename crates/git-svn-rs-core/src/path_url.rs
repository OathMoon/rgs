pub fn canonicalize_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let mut parts = Vec::new();

    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }

    parts.join("/")
}

pub fn canonicalize_url(url: &str) -> String {
    if let Some((scheme, rest)) = url.split_once("://") {
        if let Some((host, path)) = rest.split_once('/') {
            let path = canonicalize_path(path);
            return if path.is_empty() {
                format!("{scheme}://{host}")
            } else {
                format!("{scheme}://{host}/{path}")
            };
        }
        return url.trim_end_matches('/').to_string();
    }

    canonicalize_path(url)
}

pub fn join_paths<I, S>(parts: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let joined = parts
        .into_iter()
        .filter_map(|part| {
            let part = part.as_ref().trim_matches('/');
            (!part.is_empty()).then(|| part.to_string())
        })
        .collect::<Vec<_>>()
        .join("/");

    canonicalize_path(&joined)
}

pub fn add_path_to_url(url: &str, path: &str) -> String {
    let base = url.trim_end_matches('/');
    let path = canonicalize_path(path.trim_start_matches('/'));

    if path.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{path}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvnUrlProfile {
    Mock,
    File,
    Svn,
    Http,
    SvnSsh,
    Unsupported,
}

pub fn svn_url_profile(url: &str) -> SvnUrlProfile {
    let Some((scheme, _)) = url.split_once("://") else {
        return SvnUrlProfile::Unsupported;
    };
    match scheme.to_ascii_lowercase().as_str() {
        "mock" => SvnUrlProfile::Mock,
        "file" => SvnUrlProfile::File,
        "svn" => SvnUrlProfile::Svn,
        "http" | "https" => SvnUrlProfile::Http,
        "svn+ssh" => SvnUrlProfile::SvnSsh,
        _ => SvnUrlProfile::Unsupported,
    }
}

pub fn validate_fetch_url(url: &str) -> Result<(), String> {
    match svn_url_profile(url) {
        SvnUrlProfile::Mock | SvnUrlProfile::File | SvnUrlProfile::Svn | SvnUrlProfile::SvnSsh => {
            Ok(())
        }
        SvnUrlProfile::Http => Err(format!(
            "HTTP(S) SVN fetch is deferred until the remote protocol profile is validated: {url}"
        )),
        SvnUrlProfile::Unsupported => Err(format!("unsupported SVN URL scheme: {url}")),
    }
}

pub fn validate_dcommit_write_urls(target_url: &str, tracked_url: &str) -> Result<(), String> {
    let target = svn_url_profile(target_url);
    let tracked = svn_url_profile(tracked_url);
    if target == SvnUrlProfile::Mock && tracked == SvnUrlProfile::Mock {
        return Ok(());
    }
    if matches!(target, SvnUrlProfile::File | SvnUrlProfile::Svn)
        && matches!(tracked, SvnUrlProfile::File | SvnUrlProfile::Svn)
    {
        return Ok(());
    }
    match target {
        SvnUrlProfile::Http => Err(format!(
            "HTTP(S) SVN dcommit write-back is not implemented; refusing before recovery or write setup: {target_url}"
        )),
        SvnUrlProfile::SvnSsh => Err(format!(
            "svn+ssh SVN dcommit write-back is not implemented; refusing before recovery or write setup: {target_url}"
        )),
        SvnUrlProfile::Unsupported => {
            Err(format!("unsupported SVN dcommit URL scheme: {target_url}"))
        }
        _ => match tracked {
            SvnUrlProfile::Http => Err(format!(
                "the tracked HTTP(S) SVN profile is unvalidated for dcommit recovery: {tracked_url}"
            )),
            SvnUrlProfile::SvnSsh => Err(format!(
                "the tracked svn+ssh SVN profile is unvalidated for dcommit recovery: {tracked_url}"
            )),
            SvnUrlProfile::Unsupported => {
                Err(format!("unsupported tracked SVN URL scheme: {tracked_url}"))
            }
            _ => Err(format!(
                "dcommit target and tracked SVN URLs use incompatible profiles: {target_url} and {tracked_url}"
            )),
        },
    }
}
