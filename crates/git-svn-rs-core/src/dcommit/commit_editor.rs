use std::collections::BTreeSet;

use crate::dcommit::PropertyMapper;
use crate::dcommit::diff_planner::{PlannedChange, PlannedChangeKind, normalize_commit_path};
use crate::svn::editor::CommitEditor;

#[derive(Debug, Clone, Default)]
pub struct PathEnsurer {
    ensured: BTreeSet<String>,
}

impl PathEnsurer {
    pub fn ensure_dir(&mut self, editor: &mut dyn CommitEditor, path: &str) -> Result<(), String> {
        let path = normalize_commit_path(path)?;
        if self.ensured.insert(path.clone()) {
            editor.ensure_path(&path)?;
        }
        Ok(())
    }

    pub fn ensure_parent_dirs(
        &mut self,
        editor: &mut dyn CommitEditor,
        path: &str,
    ) -> Result<(), String> {
        let path = normalize_commit_path(path)?;
        let components: Vec<_> = path.split('/').collect();
        let mut current = String::new();
        for component in components.iter().take(components.len().saturating_sub(1)) {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(component);
            self.ensure_dir(editor, &current)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct SvnCommitEditor {
    property_mapper: PropertyMapper,
}

impl SvnCommitEditor {
    pub fn new(property_mapper: PropertyMapper) -> Self {
        Self { property_mapper }
    }

    pub fn apply(
        &self,
        editor: &mut dyn CommitEditor,
        changes: impl IntoIterator<Item = PlannedChange>,
    ) -> Result<u32, String> {
        let mut path_ensurer = PathEnsurer::default();

        for change in changes {
            match change.kind {
                PlannedChangeKind::EnsureDir => {
                    path_ensurer.ensure_dir(editor, &change.path)?;
                }
                PlannedChangeKind::AddFile => {
                    path_ensurer.ensure_parent_dirs(editor, &change.path)?;
                    let content = change.content.as_deref().unwrap_or_default();
                    editor.add_file(&change.path, content)?;
                    self.apply_file_properties(editor, &change)?;
                }
                PlannedChangeKind::ModifyFile => {
                    let content = change.content.as_deref().unwrap_or_default();
                    editor.open_file(&change.path, content)?;
                    self.apply_file_properties(editor, &change)?;
                }
                PlannedChangeKind::Delete => {
                    editor.delete_entry(&change.path)?;
                }
            }
        }

        editor.close_edit()
    }

    fn apply_file_properties(
        &self,
        editor: &mut dyn CommitEditor,
        change: &PlannedChange,
    ) -> Result<(), String> {
        for (name, value) in self
            .property_mapper
            .file_properties(change.symlink, change.executable)
        {
            editor.change_file_prop(&change.path, &name, value.as_deref())?;
        }
        Ok(())
    }
}
