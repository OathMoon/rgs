use std::collections::BTreeSet;

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

    pub fn with_executable(mut self, executable: bool) -> Self {
        self.executable = executable;
        self
    }

    pub fn with_symlink(mut self, symlink: bool) -> Self {
        self.symlink = symlink;
        self
    }

    fn new(path: impl Into<String>, kind: PlannedChangeKind, content: Option<Vec<u8>>) -> Self {
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
pub enum PlannedChangeKind {
    EnsureDir,
    AddFile,
    ModifyFile,
    Delete,
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
