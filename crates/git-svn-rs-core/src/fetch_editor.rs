use std::collections::BTreeMap;

use crate::fast_import::{FastImportCommit, FileChange};
use crate::git::{GitCli, GitTreeFile};
use crate::svn::editor::FetchEditor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchCommitPlan {
    pub mark: u32,
    pub refname: String,
    pub author: String,
    pub committer: String,
    pub timestamp: i64,
    pub message: String,
    pub parent_mark: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    path: String,
    file: PlannedFile,
}

impl TreeEntry {
    pub fn file(path: impl Into<String>, mode: impl AsRef<str>, content: impl AsRef<[u8]>) -> Self {
        Self {
            path: normalize_path(path.into()),
            file: PlannedFile::from_mode(mode.as_ref(), content.as_ref().to_vec()),
        }
    }

    pub fn from_git_file(file: GitTreeFile) -> Self {
        Self::file(file.path, file.mode, file.content)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SvnFetchEditor {
    plan: FetchCommitPlan,
    path_prefix: String,
    base_tree: BTreeMap<String, PlannedFile>,
    changes: BTreeMap<String, PlannedChange>,
    closed: bool,
}

impl SvnFetchEditor {
    pub fn new(plan: FetchCommitPlan) -> Self {
        Self::with_base_tree(plan, Vec::new())
    }

    pub fn with_base_tree(plan: FetchCommitPlan, entries: Vec<TreeEntry>) -> Self {
        Self {
            plan,
            path_prefix: String::new(),
            base_tree: entries
                .into_iter()
                .map(|entry| (entry.path, entry.file))
                .collect(),
            changes: BTreeMap::new(),
            closed: false,
        }
    }

    pub fn from_git_ref(
        git: &GitCli,
        plan: FetchCommitPlan,
        refname: &str,
    ) -> Result<Self, String> {
        let entries = git
            .tree_files(refname)?
            .into_iter()
            .map(TreeEntry::from_git_file)
            .collect();
        Ok(Self::with_base_tree(plan, entries))
    }

    pub fn with_path_prefix(mut self, prefix: impl AsRef<str>) -> Self {
        self.path_prefix = normalize_path(prefix);
        self
    }

    pub fn into_commit(self) -> Result<FastImportCommit, String> {
        if !self.closed {
            return Err("fetch edit is not closed".to_string());
        }

        Ok(FastImportCommit {
            mark: self.plan.mark,
            refname: self.plan.refname,
            author: self.plan.author,
            committer: self.plan.committer,
            timestamp: self.plan.timestamp,
            message: self.plan.message,
            parent_mark: self.plan.parent_mark,
            parent_ref: None,
            changes: self
                .changes
                .into_iter()
                .map(|(path, change)| match change {
                    PlannedChange::Modify(file) => FileChange::Modify {
                        path,
                        mode: file.mode().to_string(),
                        content: file.git_content(),
                    },
                    PlannedChange::Delete => FileChange::Delete { path },
                })
                .collect(),
        })
    }

    fn modify_file(&mut self, path: &str) -> &mut PlannedFile {
        let path = self.git_path(path);
        if !matches!(self.changes.get(&path), Some(PlannedChange::Modify(_))) {
            let file = self
                .base_tree
                .get(&path)
                .cloned()
                .unwrap_or_else(PlannedFile::regular);
            self.changes
                .insert(path.clone(), PlannedChange::Modify(file));
        }

        match self.changes.get_mut(&path) {
            Some(PlannedChange::Modify(file)) => file,
            Some(PlannedChange::Delete) | None => unreachable!("modify_file inserts a file change"),
        }
    }

    fn copy_file(&mut self, destination: &str, source: &str) -> Result<(), String> {
        let source = self.git_path(source);
        let file = self
            .file_at(&source)
            .ok_or_else(|| format!("copy source file not found: {source}"))?;
        self.changes
            .insert(self.git_path(destination), PlannedChange::Modify(file));
        Ok(())
    }

    fn copy_directory(&mut self, destination: &str, source: &str) {
        let destination = self.git_path(destination);
        let source = self.git_path(source);
        let source_prefix = child_prefix(&source);
        let copies: Vec<_> = self
            .visible_files()
            .into_iter()
            .filter_map(|(path, file)| {
                path.strip_prefix(&source_prefix).map(|relative| {
                    (
                        join_path(&destination, relative),
                        PlannedChange::Modify(file.clone()),
                    )
                })
            })
            .collect();

        for (path, change) in copies {
            self.changes.insert(path, change);
        }
    }

    fn file_at(&self, path: &str) -> Option<PlannedFile> {
        match self.changes.get(path) {
            Some(PlannedChange::Modify(file)) => Some(file.clone()),
            Some(PlannedChange::Delete) => None,
            None => self.base_tree.get(path).cloned(),
        }
    }

    fn visible_files(&self) -> BTreeMap<String, PlannedFile> {
        let mut files = self.base_tree.clone();
        for (path, change) in &self.changes {
            match change {
                PlannedChange::Modify(file) => {
                    files.insert(path.clone(), file.clone());
                }
                PlannedChange::Delete => {
                    remove_path_and_children(&mut files, path);
                }
            }
        }
        files
    }

    fn git_path(&self, path: &str) -> String {
        let path = normalize_path(path);
        if self.path_prefix.is_empty() {
            return path;
        }
        path.strip_prefix(&self.path_prefix)
            .map(|relative| relative.trim_start_matches('/').to_string())
            .unwrap_or(path)
    }
}

impl FetchEditor for SvnFetchEditor {
    fn open_root(&mut self, _revision: u32) -> Result<(), String> {
        self.closed = false;
        Ok(())
    }

    fn add_directory(&mut self, path: &str, copy_from: Option<(&str, u32)>) -> Result<(), String> {
        if let Some((source, _revision)) = copy_from {
            self.copy_directory(path, source);
        }
        Ok(())
    }

    fn add_file(&mut self, path: &str, copy_from: Option<(&str, u32)>) -> Result<(), String> {
        if let Some((source, _revision)) = copy_from {
            self.copy_file(path, source)
        } else {
            self.modify_file(path);
            Ok(())
        }
    }

    fn delete_entry(&mut self, path: &str, _revision: u32) -> Result<(), String> {
        let path = self.git_path(path);
        remove_path_and_children(&mut self.changes, &path);
        self.changes.insert(path, PlannedChange::Delete);
        Ok(())
    }

    fn change_file_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        let file = self.modify_file(path);
        match name {
            "svn:executable" => file.executable = value.is_some(),
            "svn:special" => file.special = value.is_some(),
            _ => {}
        }
        Ok(())
    }

    fn apply_textdelta(&mut self, path: &str, content: &[u8]) -> Result<(), String> {
        self.modify_file(path).content = content.to_vec();
        Ok(())
    }

    fn close_edit(&mut self) -> Result<(), String> {
        self.closed = true;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PlannedChange {
    Modify(PlannedFile),
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlannedFile {
    content: Vec<u8>,
    executable: bool,
    special: bool,
}

impl PlannedFile {
    fn regular() -> Self {
        Self {
            content: Vec::new(),
            executable: false,
            special: false,
        }
    }

    fn from_mode(mode: &str, content: Vec<u8>) -> Self {
        Self {
            content,
            executable: mode == "100755",
            special: mode == "120000",
        }
    }

    fn mode(&self) -> &'static str {
        if self.special {
            "120000"
        } else if self.executable {
            "100755"
        } else {
            "100644"
        }
    }

    fn git_content(&self) -> Vec<u8> {
        if self.special {
            self.content
                .strip_prefix(b"link ")
                .unwrap_or(&self.content)
                .to_vec()
        } else {
            self.content.clone()
        }
    }
}

fn remove_path_and_children<T>(map: &mut BTreeMap<String, T>, path: &str) {
    let prefix = child_prefix(path);
    map.retain(|candidate, _| candidate != path && !candidate.starts_with(&prefix));
}

fn normalize_path(path: impl AsRef<str>) -> String {
    path.as_ref().trim_matches('/').to_string()
}

fn child_prefix(path: &str) -> String {
    if path.is_empty() {
        String::new()
    } else {
        format!("{path}/")
    }
}

fn join_path(base: &str, child: &str) -> String {
    if base.is_empty() {
        child.to_string()
    } else if child.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{child}")
    }
}
