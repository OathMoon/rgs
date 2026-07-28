use crate::glob_spec::GlobSpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappingKind {
    Fetch,
    Branches,
    Tags,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefMapping {
    pub kind: MappingKind,
    pub svn_path: String,
    pub git_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutMappings {
    pub fetch: Vec<RefMapping>,
    pub branches: Vec<RefMapping>,
    pub tags: Vec<RefMapping>,
}

pub fn build_single_path(prefix: &str) -> LayoutMappings {
    LayoutMappings {
        fetch: vec![RefMapping {
            kind: MappingKind::Fetch,
            svn_path: String::new(),
            git_ref: format!("refs/remotes/{prefix}git-svn"),
        }],
        branches: Vec::new(),
        tags: Vec::new(),
    }
}

pub fn build_standard_layout(prefix: &str) -> LayoutMappings {
    let prefix = if prefix.is_empty() { "origin/" } else { prefix };
    build_standard_layout_with_prefix(prefix)
}

pub fn build_from_layout_args(
    stdlayout: bool,
    trunk: Option<&str>,
    branches: &[String],
    tags: &[String],
    prefix: Option<&str>,
) -> Result<LayoutMappings, String> {
    let has_layout_args = stdlayout || trunk.is_some() || !branches.is_empty() || !tags.is_empty();
    let prefix = prefix.unwrap_or(if has_layout_args { "origin/" } else { "" });

    if trunk.is_some() || !branches.is_empty() || !tags.is_empty() {
        let mut mappings = if stdlayout {
            build_standard_layout_with_prefix(prefix)
        } else {
            LayoutMappings {
                fetch: Vec::new(),
                branches: Vec::new(),
                tags: Vec::new(),
            }
        };

        if let Some(trunk) = trunk {
            mappings.fetch = vec![RefMapping {
                kind: MappingKind::Fetch,
                svn_path: trim_path(trunk),
                git_ref: format!("refs/remotes/{prefix}trunk"),
            }];
        }

        if !branches.is_empty() {
            mappings.branches.clear();
            for branch in branches {
                let svn_path = validate_glob(branch)?;
                mappings.branches.push(RefMapping {
                    kind: MappingKind::Branches,
                    svn_path,
                    git_ref: format!("refs/remotes/{prefix}*"),
                });
            }
        }

        if !tags.is_empty() {
            mappings.tags.clear();
            for tag in tags {
                let svn_path = validate_glob(tag)?;
                mappings.tags.push(RefMapping {
                    kind: MappingKind::Tags,
                    svn_path,
                    git_ref: format!("refs/remotes/{prefix}tags/*"),
                });
            }
        }

        return Ok(mappings);
    }

    if stdlayout {
        return Ok(build_standard_layout_with_prefix(prefix));
    }

    Ok(build_single_path(prefix))
}

fn build_standard_layout_with_prefix(prefix: &str) -> LayoutMappings {
    LayoutMappings {
        fetch: vec![RefMapping {
            kind: MappingKind::Fetch,
            svn_path: "trunk".to_string(),
            git_ref: format!("refs/remotes/{prefix}trunk"),
        }],
        branches: vec![RefMapping {
            kind: MappingKind::Branches,
            svn_path: "branches/*".to_string(),
            git_ref: format!("refs/remotes/{prefix}*"),
        }],
        tags: vec![RefMapping {
            kind: MappingKind::Tags,
            svn_path: "tags/*".to_string(),
            git_ref: format!("refs/remotes/{prefix}tags/*"),
        }],
    }
}

fn validate_glob(glob: &str) -> Result<String, String> {
    let trimmed = trim_path(glob);
    GlobSpec::new(&trimmed, true)?;
    Ok(trimmed)
}

fn trim_path(path: &str) -> String {
    path.trim_matches('/').to_string()
}

pub fn sanitize_refname(refname: &str) -> Result<String, String> {
    if !refname.starts_with("refs/") {
        return Err(format!(
            "git-svn destination {refname:?} must begin with refs/"
        ));
    }
    if refname.ends_with('/') {
        return Err(format!(
            "ref {refname:?} ends with a trailing slash, which Git and Subversion do not permit"
        ));
    }

    let mut escaped = String::with_capacity(refname.len());
    for character in refname.chars() {
        if matches!(
            character,
            ' ' | '%' | '~' | '^' | ':' | '?' | '*' | '[' | '\t' | '\\'
        ) || character.is_ascii_control()
        {
            escaped.push_str(&format!("%{:02X}", u32::from(character)));
        } else {
            escaped.push(character);
        }
    }

    let components = escaped
        .split('/')
        .map(|component| {
            let mut component = component.to_string();
            if component.starts_with('.') {
                component.replace_range(..1, "%2E");
            }
            component = component.replace("..", "%2E%2E");
            if component.ends_with(".lock") {
                let dot = component.len() - ".lock".len();
                component.replace_range(dot..dot + 1, "%2E");
            } else if component.ends_with('.') {
                component.replace_range(component.len() - 1.., "%2E");
            }
            component.replace("@{", "%40{")
        })
        .collect::<Vec<_>>()
        .join("/");
    Ok(components)
}

pub fn desanitize_refname(refname: &str) -> String {
    let bytes = refname.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Ok(value) = u8::from_str_radix(&refname[index + 1..index + 3], 16)
        {
            decoded.push(value);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use super::{desanitize_refname, sanitize_refname};

    #[test]
    fn refname_sanitization_matches_frozen_git_svn_rules() {
        for (raw, expected) in [
            ("refs/remotes/topic name", "refs/remotes/topic%20name"),
            ("refs/remotes/.topic", "refs/remotes/%2Etopic"),
            ("refs/remotes/topic..next", "refs/remotes/topic%2E%2Enext"),
            ("refs/remotes/topic.lock", "refs/remotes/topic%2Elock"),
            ("refs/remotes/topic.", "refs/remotes/topic%2E"),
            ("refs/remotes/topic@{1", "refs/remotes/topic%40{1"),
            ("refs/remotes/百分百", "refs/remotes/百分百"),
        ] {
            assert_eq!(sanitize_refname(raw).unwrap(), expected);
            assert_eq!(desanitize_refname(expected), raw);
        }
    }

    #[test]
    fn trailing_slash_is_rejected() {
        assert!(sanitize_refname("refs/remotes/topic/").is_err());
    }

    #[test]
    fn destination_outside_refs_is_rejected() {
        assert!(sanitize_refname("../../escape").is_err());
    }
}
