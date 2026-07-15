use std::collections::BTreeMap;

use super::editor::CommitEditor;
use super::editor::FetchEditor;
use super::ra::{DirEntry, DirListing, RaSession, SvnNodeKind};
use super::{CommitRecord, RevisionEvent, SvnBackend};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockCommit {
    pub record: CommitRecord,
    pub revision: u32,
    pub operations: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MockSvnBackend {
    uuid: String,
    revisions: Vec<RevisionEvent>,
    commits: Vec<MockCommit>,
}

impl MockSvnBackend {
    pub fn new(uuid: impl Into<String>, revisions: Vec<RevisionEvent>) -> Self {
        Self {
            uuid: uuid.into(),
            revisions,
            commits: Vec::new(),
        }
    }

    pub fn commit_editor(&mut self, record: CommitRecord) -> MockCommitEditor<'_> {
        MockCommitEditor {
            backend: self,
            record,
            operations: Vec::new(),
            closed: false,
        }
    }

    pub fn commits(&self) -> &[MockCommit] {
        &self.commits
    }
}

impl SvnBackend for MockSvnBackend {
    fn uuid(&self) -> Result<String, String> {
        Ok(self.uuid.clone())
    }

    fn latest_revnum(&self) -> Result<u32, String> {
        let latest_revision = self
            .revisions
            .last()
            .map(|revision| revision.revision)
            .unwrap_or(0);
        let latest_commit = self
            .commits
            .last()
            .map(|commit| commit.revision)
            .unwrap_or(0);
        Ok(latest_revision.max(latest_commit))
    }

    fn log(&self, start: u32, end: u32) -> Result<Vec<RevisionEvent>, String> {
        Ok(self
            .revisions
            .iter()
            .filter(|revision| revision.revision >= start && revision.revision <= end)
            .cloned()
            .collect())
    }
}

#[derive(Debug)]
pub struct MockCommitEditor<'a> {
    backend: &'a mut MockSvnBackend,
    record: CommitRecord,
    operations: Vec<String>,
    closed: bool,
}

impl CommitEditor for MockCommitEditor<'_> {
    fn ensure_path(&mut self, path: &str) -> Result<(), String> {
        self.operations.push(format!("ensure {path}"));
        Ok(())
    }

    fn add_file(&mut self, path: &str, content: &[u8]) -> Result<(), String> {
        self.operations
            .push(format!("add {path} {}", String::from_utf8_lossy(content)));
        Ok(())
    }

    fn open_file(&mut self, path: &str, content: &[u8]) -> Result<(), String> {
        self.operations
            .push(format!("open {path} {}", String::from_utf8_lossy(content)));
        Ok(())
    }

    fn delete_entry(&mut self, path: &str) -> Result<(), String> {
        self.operations.push(format!("delete {path}"));
        Ok(())
    }

    fn copy_file(
        &mut self,
        source_path: &str,
        source_revision: u32,
        path: &str,
    ) -> Result<(), String> {
        self.operations
            .push(format!("copy {source_path}@{source_revision} {path}"));
        Ok(())
    }

    fn move_entry(
        &mut self,
        source_path: &str,
        source_revision: u32,
        path: &str,
    ) -> Result<(), String> {
        self.operations
            .push(format!("move {source_path}@{source_revision} {path}"));
        Ok(())
    }

    fn change_file_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        self.operations.push(format!(
            "prop {path} {name} {}",
            value.unwrap_or("<deleted>")
        ));
        Ok(())
    }

    fn change_directory_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        self.operations.push(format!(
            "dir-prop {path} {name} {}",
            value.unwrap_or("<deleted>")
        ));
        Ok(())
    }

    fn close_edit(&mut self) -> Result<u32, String> {
        if self.closed {
            return Err("commit editor already closed".to_string());
        }
        let revision = self.record.base_revision + 1;
        self.backend.commits.push(MockCommit {
            record: self.record.clone(),
            revision,
            operations: self.operations.clone(),
        });
        self.closed = true;
        Ok(revision)
    }

    fn abort_edit(&mut self) -> Result<(), String> {
        self.closed = true;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct MockRaSession {
    url: String,
    repos_root: String,
    backend: MockSvnBackend,
}

impl MockRaSession {
    pub fn standard_fixture(uuid: impl Into<String>) -> Self {
        let revisions = vec![
            RevisionEvent {
                revision: 1,
                author: "alice".to_string(),
                message: "layout".to_string(),
                timestamp: "2026-01-01T00:00:00Z".to_string(),
                changed_paths: vec![],
            },
            RevisionEvent {
                revision: 2,
                author: "bob".to_string(),
                message: "add trunk file".to_string(),
                timestamp: "2026-01-02T00:00:00Z".to_string(),
                changed_paths: vec![],
            },
        ];

        Self {
            url: "mock://repo/trunk".to_string(),
            repos_root: "mock://repo".to_string(),
            backend: MockSvnBackend::new(uuid, revisions),
        }
    }

    fn path_kind(path: &str, revision: u32) -> Option<SvnNodeKind> {
        match (trim_path(path), revision) {
            ("", _) => Some(SvnNodeKind::Directory),
            ("trunk", 1..) => Some(SvnNodeKind::Directory),
            ("trunk/src", 2..) => Some(SvnNodeKind::Directory),
            ("trunk/src/lib.rs", 2..) => Some(SvnNodeKind::File),
            _ => None,
        }
    }
}

impl RaSession for MockRaSession {
    fn url(&self) -> &str {
        &self.url
    }

    fn repos_root(&self) -> &str {
        &self.repos_root
    }

    fn uuid(&self) -> Result<String, String> {
        self.backend.uuid()
    }

    fn latest_revnum(&self) -> Result<u32, String> {
        self.backend.latest_revnum()
    }

    fn check_path(&self, path: &str, revision: u32) -> Result<Option<SvnNodeKind>, String> {
        Ok(Self::path_kind(path, revision))
    }

    fn get_dir(&self, path: &str, revision: u32) -> Result<DirListing, String> {
        let path = trim_path(path);
        if Self::path_kind(path, revision) != Some(SvnNodeKind::Directory) {
            return Err(format!("path is not a directory at r{revision}: {path}"));
        }

        let entries = match (path, revision) {
            ("", _) => entries([("trunk", SvnNodeKind::Directory)]),
            ("trunk", 1) => BTreeMap::new(),
            ("trunk", 2..) => entries([("src", SvnNodeKind::Directory)]),
            ("trunk/src", 2..) => entries([("lib.rs", SvnNodeKind::File)]),
            _ => BTreeMap::new(),
        };

        Ok(DirListing {
            entries,
            properties: BTreeMap::new(),
        })
    }

    fn get_log(&self, paths: &[&str], start: u32, end: u32) -> Result<Vec<RevisionEvent>, String> {
        let logs = self.backend.log(start, end)?;
        if paths.is_empty() {
            return Ok(logs);
        }

        Ok(logs
            .into_iter()
            .filter(|revision| {
                revision.changed_paths.is_empty()
                    || revision.changed_paths.iter().any(|changed_path| {
                        paths
                            .iter()
                            .any(|path| trim_path(&changed_path.path).starts_with(trim_path(path)))
                    })
            })
            .collect())
    }

    fn do_update(
        &self,
        path: &str,
        revision: u32,
        editor: &mut dyn FetchEditor,
    ) -> Result<(), String> {
        drive_standard_editor(path, revision, None, editor)
    }

    fn do_switch(
        &self,
        path: &str,
        revision: u32,
        switch_url: &str,
        editor: &mut dyn FetchEditor,
    ) -> Result<(), String> {
        drive_standard_editor(path, revision, Some(switch_url), editor)
    }
}

fn drive_standard_editor(
    path: &str,
    revision: u32,
    switch_url: Option<&str>,
    editor: &mut dyn FetchEditor,
) -> Result<(), String> {
    let path = trim_path(path);
    editor.open_root(revision)?;
    editor.add_directory(path, switch_url.map(|url| (url, revision)))?;
    editor.add_directory(join_path(path, "src").as_str(), None)?;
    editor.add_file(join_path(path, "src/lib.rs").as_str(), None)?;
    editor.change_file_prop(
        join_path(path, "src/lib.rs").as_str(),
        "svn:eol-style",
        Some("LF"),
    )?;
    editor.apply_textdelta(
        join_path(path, "src/lib.rs").as_str(),
        b"pub fn answer() -> u8 { 42 }\n",
    )?;
    editor.delete_entry(
        join_path(path, "obsolete.txt").as_str(),
        revision.saturating_sub(1),
    )?;
    editor.close_edit()
}

fn entries<const N: usize>(items: [(&str, SvnNodeKind); N]) -> BTreeMap<String, DirEntry> {
    items
        .into_iter()
        .map(|(name, kind)| (name.to_string(), DirEntry { kind }))
        .collect()
}

fn trim_path(path: &str) -> &str {
    path.trim_matches('/')
}

fn join_path(base: &str, child: &str) -> String {
    if base.is_empty() {
        child.to_string()
    } else {
        format!("{base}/{child}")
    }
}
