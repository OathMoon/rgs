use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DcommitPlan {
    pub target: DcommitTarget,
    pub base_revision: u32,
    pub git_commit: String,
    pub message: String,
    pub author: Option<String>,
    pub root_properties: Vec<PropertyChange>,
    pub changes: Vec<PlannedChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DcommitTarget {
    pub url: String,
    pub repository_root: String,
    pub repository_uuid: String,
    pub git_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyChange {
    pub name: String,
    pub value: Option<String>,
}

impl PropertyChange {
    pub fn set(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: Some(value.into()),
        }
    }

    pub fn delete(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopySource {
    pub path: String,
    pub revision: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeMetadata {
    pub old_mode: String,
    pub new_mode: String,
    pub old_oid: String,
    pub new_oid: String,
    pub similarity: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedCommit {
    pub changes: Vec<PlannedChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedChange {
    pub path: String,
    pub kind: PlannedChangeKind,
    pub content: Option<Vec<u8>>,
    pub executable: bool,
    pub symlink: bool,
    pub source: Option<CopySource>,
    pub properties: Vec<PropertyChange>,
    pub metadata: Option<ChangeMetadata>,
}

impl PlannedChange {
    pub fn ensure_dir(path: impl Into<String>) -> Self {
        Self::new(path, PlannedChangeKind::EnsureDir, None)
    }

    pub fn add_file(path: impl Into<String>, content: impl AsRef<[u8]>) -> Self {
        Self::new(
            path,
            PlannedChangeKind::AddFile,
            Some(content.as_ref().to_vec()),
        )
    }

    pub fn modify_file(path: impl Into<String>, content: impl AsRef<[u8]>) -> Self {
        Self::new(
            path,
            PlannedChangeKind::ModifyFile,
            Some(content.as_ref().to_vec()),
        )
    }

    pub fn delete(path: impl Into<String>) -> Self {
        Self::new(path, PlannedChangeKind::Delete, None)
    }

    pub fn copy_file(
        source_path: impl Into<String>,
        source_revision: u32,
        path: impl Into<String>,
        content: impl AsRef<[u8]>,
    ) -> Self {
        let mut change = Self::new(
            path,
            PlannedChangeKind::CopyFile,
            Some(content.as_ref().to_vec()),
        );
        change.source = Some(CopySource {
            path: source_path.into(),
            revision: source_revision,
        });
        change
    }

    pub fn move_entry(
        source_path: impl Into<String>,
        source_revision: u32,
        path: impl Into<String>,
        content: impl AsRef<[u8]>,
    ) -> Self {
        let mut change = Self::new(
            path,
            PlannedChangeKind::Move,
            Some(content.as_ref().to_vec()),
        );
        change.source = Some(CopySource {
            path: source_path.into(),
            revision: source_revision,
        });
        change
    }

    pub fn with_executable(mut self, executable: bool) -> Self {
        self.executable = executable;
        self
    }

    pub fn with_symlink(mut self, symlink: bool) -> Self {
        self.symlink = symlink;
        self
    }

    pub fn with_property(mut self, property: PropertyChange) -> Self {
        self.properties.push(property);
        self
    }

    pub fn with_metadata(mut self, metadata: ChangeMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    fn new(path: impl Into<String>, kind: PlannedChangeKind, content: Option<Vec<u8>>) -> Self {
        Self {
            path: path.into(),
            kind,
            content,
            executable: false,
            symlink: false,
            source: None,
            properties: Vec::new(),
            metadata: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedChangeKind {
    EnsureDir,
    AddFile,
    ModifyFile,
    Delete,
    CopyFile,
    Move,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitDiffChange {
    path: String,
    kind: GitDiffChangeKind,
    content: Option<Vec<u8>>,
    executable: bool,
    symlink: bool,
}

impl GitDiffChange {
    pub fn add_file(path: impl Into<String>, content: impl AsRef<[u8]>) -> Self {
        Self::new(
            path,
            GitDiffChangeKind::AddFile,
            Some(content.as_ref().to_vec()),
        )
    }

    pub fn modify_file(path: impl Into<String>, content: impl AsRef<[u8]>) -> Self {
        Self::new(
            path,
            GitDiffChangeKind::ModifyFile,
            Some(content.as_ref().to_vec()),
        )
    }

    pub fn delete(path: impl Into<String>) -> Self {
        Self::new(path, GitDiffChangeKind::Delete, None)
    }

    pub fn with_executable(mut self, executable: bool) -> Self {
        self.executable = executable;
        self
    }

    pub fn with_symlink(mut self, symlink: bool) -> Self {
        self.symlink = symlink;
        self
    }

    fn new(path: impl Into<String>, kind: GitDiffChangeKind, content: Option<Vec<u8>>) -> Self {
        Self {
            path: path.into(),
            kind,
            content,
            executable: false,
            symlink: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GitDiffChangeKind {
    AddFile,
    ModifyFile,
    Delete,
}

#[derive(Debug, Clone, Default)]
pub struct GitDiffPlanner;

impl GitDiffPlanner {
    pub fn new() -> Self {
        Self
    }

    pub fn plan(
        &self,
        changes: impl IntoIterator<Item = GitDiffChange>,
    ) -> Result<PlannedCommit, String> {
        let mut planned = Vec::new();
        let mut ensured_dirs = BTreeSet::new();
        let mut deletes = Vec::new();

        for change in changes {
            let path = normalize_commit_path(&change.path)?;
            match change.kind {
                GitDiffChangeKind::AddFile => {
                    push_parent_dirs(&mut planned, &mut ensured_dirs, &path);
                    let mut planned_change =
                        PlannedChange::add_file(path, change.content.unwrap_or_default());
                    planned_change.executable = change.executable;
                    planned_change.symlink = change.symlink;
                    planned.push(planned_change);
                }
                GitDiffChangeKind::ModifyFile => {
                    let mut planned_change =
                        PlannedChange::modify_file(path, change.content.unwrap_or_default());
                    planned_change.executable = change.executable;
                    planned_change.symlink = change.symlink;
                    planned.push(planned_change);
                }
                GitDiffChangeKind::Delete => deletes.push(PlannedChange::delete(path)),
            }
        }

        deletes.sort_by(|left, right| {
            right
                .path
                .matches('/')
                .count()
                .cmp(&left.path.matches('/').count())
                .then_with(|| left.path.cmp(&right.path))
        });
        planned.extend(deletes);

        Ok(PlannedCommit { changes: planned })
    }
}

fn push_parent_dirs(
    planned: &mut Vec<PlannedChange>,
    ensured_dirs: &mut BTreeSet<String>,
    path: &str,
) {
    let mut current = String::new();
    let components: Vec<_> = path.split('/').collect();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(component);
        if ensured_dirs.insert(current.clone()) {
            planned.push(PlannedChange::ensure_dir(current.clone()));
        }
    }
}

pub(crate) fn normalize_commit_path(path: &str) -> Result<String, String> {
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(format!("path is outside commit root: {path}"));
    }

    let mut components = Vec::new();
    for component in path.split(['/', '\\']) {
        match component {
            "" | "." => {}
            ".." => return Err(format!("path is outside commit root: {path}")),
            value => components.push(value),
        }
    }

    if components.is_empty() {
        return Err("path must not be empty".to_string());
    }

    Ok(components.join("/"))
}
