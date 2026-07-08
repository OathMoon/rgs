use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionEvent {
    pub revision: u32,
    pub author: String,
    pub message: String,
    pub timestamp: String,
    pub changed_paths: Vec<ChangedPath>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedPath {
    pub path: String,
    pub action: ChangeAction,
    pub copy_from_path: Option<String>,
    pub copy_from_rev: Option<u32>,
    pub kind: NodeKind,
    pub properties_modified: bool,
    pub properties: BTreeMap<String, String>,
    pub content: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeAction {
    Add,
    Modify,
    Delete,
    Replace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRecord {
    pub author: String,
    pub message: String,
    pub base_revision: u32,
}
