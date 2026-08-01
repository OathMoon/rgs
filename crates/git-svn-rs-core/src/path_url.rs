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

pub fn repository_relative_url_path(repository_root: &str, target: &str) -> Result<String, String> {
    let repository_root = url::Url::parse(repository_root)
        .map_err(|error| format!("invalid SVN repository root URL {repository_root:?}: {error}"))?;
    let target = url::Url::parse(target)
        .map_err(|error| format!("invalid SVN layout URL {target:?}: {error}"))?;

    let same_authority = repository_root
        .scheme()
        .eq_ignore_ascii_case(target.scheme())
        && repository_root.username() == target.username()
        && repository_root.password() == target.password()
        && repository_root.host_str().map(str::to_ascii_lowercase)
            == target.host_str().map(str::to_ascii_lowercase)
        && repository_root.port_or_known_default() == target.port_or_known_default();
    let root_path = repository_root.path().trim_end_matches('/');
    let target_path = target.path().trim_end_matches('/');
    let relative = if same_authority && target_path == root_path {
        ""
    } else if same_authority {
        target_path
            .strip_prefix(root_path)
            .filter(|suffix| suffix.starts_with('/'))
            .map(|suffix| suffix.trim_start_matches('/'))
            .ok_or_else(|| {
                format!(
                    "SVN layout URL is outside repository root: {target} (root: {repository_root})"
                )
            })?
    } else {
        return Err(format!(
            "SVN layout URL is outside repository root: {target} (root: {repository_root})"
        ));
    };

    percent_decode_url_path(relative)
}

fn percent_decode_url_path(path: &str) -> Result<String, String> {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(format!("invalid percent escape in SVN URL path {path:?}"));
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                .map_err(|error| error.to_string())?;
            let value = u8::from_str_radix(hex, 16)
                .map_err(|_| format!("invalid percent escape %{hex} in SVN URL path {path:?}"))?;
            decoded.push(value);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded)
        .map_err(|error| format!("SVN URL path is not valid UTF-8 after decoding: {error}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvnUrlProfile {
    Mock,
    File,
    Svn,
    Http,
    Https,
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
        "http" => SvnUrlProfile::Http,
        "https" => SvnUrlProfile::Https,
        "svn+ssh" => SvnUrlProfile::SvnSsh,
        _ => SvnUrlProfile::Unsupported,
    }
}

pub fn validate_fetch_url(url: &str) -> Result<(), String> {
    match svn_url_profile(url) {
        SvnUrlProfile::Mock
        | SvnUrlProfile::File
        | SvnUrlProfile::Svn
        | SvnUrlProfile::Http
        | SvnUrlProfile::Https
        | SvnUrlProfile::SvnSsh => Ok(()),
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
    if target == SvnUrlProfile::SvnSsh && tracked == SvnUrlProfile::SvnSsh {
        return Ok(());
    }
    match target {
        SvnUrlProfile::Http | SvnUrlProfile::Https => Err(format!(
            "HTTP(S) SVN dcommit write-back is not implemented; refusing before recovery or write setup: {target_url}"
        )),
        SvnUrlProfile::SvnSsh => Err(format!(
            "svn+ssh SVN dcommit target requires a matching tracked svn+ssh profile: {target_url}"
        )),
        SvnUrlProfile::Unsupported => {
            Err(format!("unsupported SVN dcommit URL scheme: {target_url}"))
        }
        _ => match tracked {
            SvnUrlProfile::Http | SvnUrlProfile::Https => Err(format!(
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
