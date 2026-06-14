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
