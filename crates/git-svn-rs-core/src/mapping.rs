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

    if stdlayout && trunk.is_none() && branches.is_empty() && tags.is_empty() {
        return Ok(build_standard_layout_with_prefix(prefix));
    }

    if trunk.is_some() || !branches.is_empty() || !tags.is_empty() {
        let mut mappings = LayoutMappings {
            fetch: Vec::new(),
            branches: Vec::new(),
            tags: Vec::new(),
        };

        mappings.fetch.push(RefMapping {
            kind: MappingKind::Fetch,
            svn_path: trim_path(trunk.unwrap_or("trunk")),
            git_ref: format!("refs/remotes/{prefix}trunk"),
        });

        for branch in branches {
            let svn_path = validate_glob(branch)?;
            mappings.branches.push(RefMapping {
                kind: MappingKind::Branches,
                svn_path,
                git_ref: format!("refs/remotes/{prefix}*"),
            });
        }

        for tag in tags {
            let svn_path = validate_glob(tag)?;
            mappings.tags.push(RefMapping {
                kind: MappingKind::Tags,
                svn_path,
                git_ref: format!("refs/remotes/{prefix}tags/*"),
            });
        }

        return Ok(mappings);
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
