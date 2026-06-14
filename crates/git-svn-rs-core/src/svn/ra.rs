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
    fn do_switch(
        &self,
        path: &str,
        revision: u32,
        switch_url: &str,
        editor: &mut dyn FetchEditor,
    ) -> Result<(), String>;
}
