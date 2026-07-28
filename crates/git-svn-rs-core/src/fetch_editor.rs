use std::collections::{BTreeMap, BTreeSet};

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
    pub timezone_offset: String,
    pub message: String,
    pub parent_mark: Option<u32>,
    pub parent_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    path: String,
    file: PlannedFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchEditResult {
    pub commit: FastImportCommit,
    pub unhandled: UnhandledMetadata,
    pub owned_placeholders: BTreeSet<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnhandledMetadata {
    directory_properties: BTreeMap<String, BTreeMap<String, Option<Vec<u8>>>>,
    file_properties: BTreeMap<String, BTreeMap<String, Option<Vec<u8>>>>,
    absent_directories: Vec<String>,
    absent_files: Vec<String>,
    empty_directories: BTreeMap<String, bool>,
}

impl UnhandledMetadata {
    pub fn is_empty(&self) -> bool {
        self.lines().is_empty()
    }

    pub fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        append_property_lines(&mut lines, "dir_prop", &self.directory_properties);
        append_property_lines(&mut lines, "file_prop", &self.file_properties);

        let mut absent_files = self.absent_files.clone();
        absent_files.sort();
        for path in absent_files {
            lines.push(format!("  absent_file: {}", uri_encode(&path)));
        }

        let mut absent_directories = self.absent_directories.clone();
        absent_directories.sort();
        for path in absent_directories {
            lines.push(format!("  absent_directory: {}", uri_encode(&path)));
        }
        for (path, present) in &self.empty_directories {
            let action = if *present { '+' } else { '-' };
            lines.push(format!("  {action}empty_dir: {}", uri_encode(path)));
        }
        lines
    }

    fn set_empty_directory(&mut self, path: String, present: bool) {
        self.empty_directories.insert(path, present);
    }
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
    directories: BTreeSet<String>,
    touched_files: BTreeSet<String>,
    owned_placeholders: BTreeSet<String>,
    unhandled: UnhandledMetadata,
    closed: bool,
}

impl SvnFetchEditor {
    pub fn new(plan: FetchCommitPlan) -> Self {
        Self::with_base_tree(plan, Vec::new())
    }

    pub fn with_base_tree(plan: FetchCommitPlan, entries: Vec<TreeEntry>) -> Self {
        let base_tree = entries
            .into_iter()
            .map(|entry| (entry.path, entry.file))
            .collect::<BTreeMap<_, _>>();
        Self {
            plan,
            path_prefix: String::new(),
            directories: directories_for_files(base_tree.keys()),
            base_tree,
            changes: BTreeMap::new(),
            touched_files: BTreeSet::new(),
            owned_placeholders: BTreeSet::new(),
            unhandled: UnhandledMetadata::default(),
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

    pub fn with_owned_placeholders(
        mut self,
        owned_placeholders: impl IntoIterator<Item = String>,
    ) -> Self {
        self.owned_placeholders = owned_placeholders
            .into_iter()
            .map(normalize_path)
            .filter(|path| !path.is_empty())
            .collect();
        self
    }

    /// Reconciles explicitly owned empty-directory placeholders from the
    /// completed base tree plus edit delta. An unowned same-named file is SVN
    /// repository content and is never removed or claimed as a placeholder.
    pub fn reconcile_empty_directories(
        &mut self,
        placeholder_filename: &str,
    ) -> Result<(), String> {
        let placeholder_filename = normalize_path(placeholder_filename);
        if placeholder_filename.is_empty() || placeholder_filename.contains('/') {
            return Err("empty-directory placeholder must be a single filename".to_string());
        }

        let mut files = self.visible_files();
        let mut directories = self.directories.iter().cloned().collect::<Vec<_>>();
        directories.sort_by_key(|path| std::cmp::Reverse(path.matches('/').count()));
        for directory in directories {
            if directory.is_empty() {
                continue;
            }
            let placeholder = join_path(&directory, &placeholder_filename);
            let prefix = child_prefix(&directory);
            let has_other_file = files
                .keys()
                .any(|path| path.starts_with(&prefix) && path != &placeholder);
            let placeholder_is_repository_file = self.touched_files.contains(&placeholder);
            let placeholder_is_owned = self.owned_placeholders.contains(&directory);

            if has_other_file || placeholder_is_repository_file {
                if placeholder_is_owned {
                    self.set_placeholder_ownership(&directory, false);
                }
                if files.contains_key(&placeholder)
                    && placeholder_is_owned
                    && !placeholder_is_repository_file
                {
                    files.remove(&placeholder);
                    self.changes.insert(placeholder, PlannedChange::Delete);
                }
            } else if !files.contains_key(&placeholder) {
                let file = PlannedFile::regular();
                files.insert(placeholder.clone(), file.clone());
                self.changes
                    .insert(placeholder, PlannedChange::Modify(file));
                self.set_placeholder_ownership(&directory, true);
            }
        }
        Ok(())
    }

    pub fn into_commit(self) -> Result<FastImportCommit, String> {
        Ok(self.into_result()?.commit)
    }

    pub fn into_result(self) -> Result<FetchEditResult, String> {
        if !self.closed {
            return Err("fetch edit is not closed".to_string());
        }

        let commit = FastImportCommit {
            mark: self.plan.mark,
            refname: self.plan.refname,
            author: self.plan.author,
            committer: self.plan.committer,
            timestamp: self.plan.timestamp,
            timezone_offset: self.plan.timezone_offset,
            message: self.plan.message,
            parent_mark: self.plan.parent_mark,
            parent_ref: self.plan.parent_ref,
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
        };
        Ok(FetchEditResult {
            commit,
            unhandled: self.unhandled,
            owned_placeholders: self.owned_placeholders,
        })
    }

    fn set_placeholder_ownership(&mut self, directory: &str, present: bool) {
        let changed = if present {
            self.owned_placeholders.insert(directory.to_string())
        } else {
            self.owned_placeholders.remove(directory)
        };
        if changed {
            self.unhandled
                .set_empty_directory(join_path(&self.path_prefix, directory), present);
        }
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

        let source_prefix = child_prefix(&source);
        let owned_copies = self
            .owned_placeholders
            .iter()
            .filter_map(|path| {
                if path == &source {
                    Some(destination.clone())
                } else {
                    path.strip_prefix(&source_prefix)
                        .map(|relative| join_path(&destination, relative))
                }
            })
            .collect::<Vec<_>>();
        for directory in owned_copies {
            self.set_placeholder_ownership(&directory, true);
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
        let path = self.git_path(path);
        insert_directory_and_parents(&mut self.directories, &path);
        if let Some((source, _revision)) = copy_from {
            self.copy_directory(&path, source);
        }
        Ok(())
    }

    fn add_file(&mut self, path: &str, copy_from: Option<(&str, u32)>) -> Result<(), String> {
        let touched_path = self.git_path(path);
        self.touched_files.insert(touched_path.clone());
        insert_parent_directories(&mut self.directories, &touched_path);
        if let Some((source, _revision)) = copy_from {
            self.copy_file(path, source)
        } else {
            self.changes
                .insert(touched_path, PlannedChange::Modify(PlannedFile::regular()));
            Ok(())
        }
    }

    fn add_file_with_copy_content(
        &mut self,
        path: &str,
        _copy_from: (&str, u32),
        content: &[u8],
    ) -> Result<(), String> {
        let touched_path = self.git_path(path);
        self.touched_files.insert(touched_path.clone());
        insert_parent_directories(&mut self.directories, &touched_path);
        self.changes.insert(
            touched_path,
            PlannedChange::Modify(PlannedFile {
                content: content.to_vec(),
                executable: false,
                special: false,
            }),
        );
        Ok(())
    }

    fn delete_entry(&mut self, path: &str, _revision: u32) -> Result<(), String> {
        let path = self.git_path(path);
        if path.is_empty() {
            let files = self.visible_files().into_keys().collect::<Vec<_>>();
            let owned = self.owned_placeholders.iter().cloned().collect::<Vec<_>>();
            self.changes.clear();
            self.directories.clear();
            self.touched_files.clear();
            for file in files {
                self.changes.insert(file, PlannedChange::Delete);
            }
            for directory in owned {
                self.set_placeholder_ownership(&directory, false);
            }
            return Ok(());
        }
        let removed_owned = self
            .owned_placeholders
            .iter()
            .filter(|candidate| *candidate == &path || candidate.starts_with(&child_prefix(&path)))
            .cloned()
            .collect::<Vec<_>>();
        for directory in removed_owned {
            self.set_placeholder_ownership(&directory, false);
        }
        remove_path_and_children(&mut self.changes, &path);
        remove_set_path_and_children(&mut self.directories, &path);
        remove_set_path_and_children(&mut self.touched_files, &path);
        self.changes.insert(path, PlannedChange::Delete);
        Ok(())
    }

    fn change_file_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        self.change_file_prop_bytes(path, name, value.map(str::as_bytes))
    }

    fn change_file_prop_bytes(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&[u8]>,
    ) -> Result<(), String> {
        let git_path = self.git_path(path);
        self.touched_files.insert(git_path.clone());
        insert_parent_directories(&mut self.directories, &git_path);
        let file = self.modify_file(path);
        match name {
            "svn:executable" => file.executable = value.is_some(),
            "svn:special" => file.special = value.is_some(),
            _ => {
                self.unhandled
                    .file_properties
                    .entry(normalize_path(path))
                    .or_default()
                    .insert(name.to_string(), value.map(<[u8]>::to_vec));
            }
        }
        Ok(())
    }

    fn change_directory_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        self.change_directory_prop_bytes(path, name, value.map(str::as_bytes))
    }

    fn change_directory_prop_bytes(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&[u8]>,
    ) -> Result<(), String> {
        let git_path = self.git_path(path);
        insert_directory_and_parents(&mut self.directories, &git_path);
        self.unhandled
            .directory_properties
            .entry(normalize_path(path))
            .or_default()
            .insert(name.to_string(), value.map(<[u8]>::to_vec));
        Ok(())
    }

    fn absent_directory(&mut self, path: &str) -> Result<(), String> {
        self.unhandled.absent_directories.push(normalize_path(path));
        Ok(())
    }

    fn absent_file(&mut self, path: &str) -> Result<(), String> {
        self.unhandled.absent_files.push(normalize_path(path));
        Ok(())
    }

    fn apply_textdelta(&mut self, path: &str, content: &[u8]) -> Result<(), String> {
        let path = self.git_path(path);
        self.touched_files.insert(path.clone());
        insert_parent_directories(&mut self.directories, &path);
        self.modify_file(&path).content = content.to_vec();
        Ok(())
    }

    fn close_edit(&mut self) -> Result<(), String> {
        self.closed = true;
        Ok(())
    }

    fn abort_edit(&mut self) -> Result<(), String> {
        self.closed = false;
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

fn directories_for_files<'a>(paths: impl Iterator<Item = &'a String>) -> BTreeSet<String> {
    let mut directories = BTreeSet::new();
    for path in paths {
        insert_parent_directories(&mut directories, path);
    }
    directories
}

fn insert_parent_directories(directories: &mut BTreeSet<String>, path: &str) {
    let mut parent = path.rsplit_once('/').map(|(parent, _)| parent);
    while let Some(path) = parent {
        if path.is_empty() {
            break;
        }
        directories.insert(path.to_string());
        parent = path.rsplit_once('/').map(|(parent, _)| parent);
    }
}

fn insert_directory_and_parents(directories: &mut BTreeSet<String>, path: &str) {
    if !path.is_empty() {
        directories.insert(path.to_string());
    }
    insert_parent_directories(directories, path);
}

fn remove_set_path_and_children(set: &mut BTreeSet<String>, path: &str) {
    let prefix = child_prefix(path);
    set.retain(|candidate| candidate != path && !candidate.starts_with(&prefix));
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

fn append_property_lines(
    lines: &mut Vec<String>,
    kind: &str,
    properties: &BTreeMap<String, BTreeMap<String, Option<Vec<u8>>>>,
) {
    for (path, values) in properties {
        let path = if path.is_empty() { "." } else { path };
        for (name, value) in values {
            if skip_property(name) {
                continue;
            }
            let prefix = format!("{kind}: {} {}", uri_encode(path), uri_encode(name));
            match value {
                Some(value) => lines.push(format!("  +{prefix} {}", uri_encode_bytes(value))),
                None => lines.push(format!("  -{prefix}")),
            }
        }
    }
}

fn skip_property(name: &str) -> bool {
    matches!(
        name,
        "svn:wc:ra_dav:version-url"
            | "svn:special"
            | "svn:executable"
            | "svn:entry:committed-rev"
            | "svn:entry:last-author"
            | "svn:entry:uuid"
            | "svn:entry:committed-date"
    )
}

fn uri_encode(value: &str) -> String {
    uri_encode_bytes(value.as_bytes())
}

fn uri_encode_bytes(value: &[u8]) -> String {
    let mut encoded = String::new();
    for byte in value {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'*' | b'!' | b':' | b'_' | b'.' | b'/' | b'-')
        {
            encoded.push(char::from(*byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}
