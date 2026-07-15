use std::collections::BTreeMap;

use crate::svn::RevisionEvent;
use crate::svn::editor::FetchEditor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SvnNodeKind {
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub kind: SvnNodeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirListing {
    pub entries: BTreeMap<String, DirEntry>,
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateRequest {
    pub target_revision: u32,
    /// The SVN revision represented by the editor's base tree. `None` requests
    /// an empty report for an initial import.
    pub base_revision: Option<u32>,
}

pub trait RaSession {
    fn url(&self) -> &str;
    fn repos_root(&self) -> &str;
    fn uuid(&self) -> Result<String, String>;
    fn latest_revnum(&self) -> Result<u32, String>;
    fn check_path(&self, path: &str, revision: u32) -> Result<Option<SvnNodeKind>, String>;
    fn get_dir(&self, path: &str, revision: u32) -> Result<DirListing, String>;
    fn get_log(&self, paths: &[&str], start: u32, end: u32) -> Result<Vec<RevisionEvent>, String>;
    fn do_update(
        &self,
        path: &str,
        revision: u32,
        editor: &mut dyn FetchEditor,
    ) -> Result<(), String>;
    fn do_update_from(
        &self,
        path: &str,
        request: UpdateRequest,
        editor: &mut dyn FetchEditor,
    ) -> Result<(), String> {
        self.do_update(path, request.target_revision, editor)
    }
    fn do_switch(
        &self,
        path: &str,
        revision: u32,
        switch_url: &str,
        editor: &mut dyn FetchEditor,
    ) -> Result<(), String>;
}
