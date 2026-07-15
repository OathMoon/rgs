use std::collections::BTreeSet;

use crate::dcommit::PropertyMapper;
use crate::dcommit::diff_planner::{
    DcommitPlan, PlannedChange, PlannedChangeKind, normalize_commit_path,
};
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
        self.apply_changes(editor, changes, true)
    }

    fn apply_changes(
        &self,
        editor: &mut dyn CommitEditor,
        changes: impl IntoIterator<Item = PlannedChange>,
        map_mode_properties: bool,
    ) -> Result<u32, String> {
        let mut path_ensurer = PathEnsurer::default();
        let result =
            (|| {
                for change in changes {
                    match change.kind {
                        PlannedChangeKind::EnsureDir => {
                            path_ensurer.ensure_dir(editor, &change.path)?;
                        }
                        PlannedChangeKind::AddFile => {
                            path_ensurer.ensure_parent_dirs(editor, &change.path)?;
                            let content = change.content.as_deref().unwrap_or_default();
                            editor.add_file(&change.path, content)?;
                            self.apply_file_properties(editor, &change, map_mode_properties)?;
                        }
                        PlannedChangeKind::ModifyFile => {
                            let content = change.content.as_deref().unwrap_or_default();
                            editor.open_file(&change.path, content)?;
                            self.apply_file_properties(editor, &change, map_mode_properties)?;
                        }
                        PlannedChangeKind::Delete => {
                            editor.delete_entry(&change.path)?;
                        }
                        PlannedChangeKind::CopyFile => {
                            path_ensurer.ensure_parent_dirs(editor, &change.path)?;
                            let source = change.source.as_ref().ok_or_else(|| {
                                format!("copy {} is missing its source", change.path)
                            })?;
                            editor.copy_file(&source.path, source.revision, &change.path)?;
                            let content = change.content.as_deref().ok_or_else(|| {
                                format!("copy {} is missing final content", change.path)
                            })?;
                            editor.open_file(&change.path, content)?;
                            self.apply_file_properties(editor, &change, map_mode_properties)?;
                        }
                        PlannedChangeKind::Move => {
                            path_ensurer.ensure_parent_dirs(editor, &change.path)?;
                            let source = change.source.as_ref().ok_or_else(|| {
                                format!("move {} is missing its source", change.path)
                            })?;
                            editor.move_entry(&source.path, source.revision, &change.path)?;
                            let content = change.content.as_deref().ok_or_else(|| {
                                format!("move {} is missing final content", change.path)
                            })?;
                            editor.open_file(&change.path, content)?;
                            self.apply_file_properties(editor, &change, map_mode_properties)?;
                        }
                    }
                }
                Ok::<(), String>(())
            })();

        if let Err(error) = result {
            return match editor.abort_edit() {
                Ok(()) => Err(error),
                Err(abort_error) => Err(format!("{error}; abort failed: {abort_error}")),
            };
        }

        editor.close_edit()
    }

    pub fn apply_plan(
        &self,
        editor: &mut dyn CommitEditor,
        plan: &DcommitPlan,
    ) -> Result<u32, String> {
        for property in &plan.root_properties {
            if let Err(error) =
                editor.change_directory_prop("", &property.name, property.value.as_deref())
            {
                return match editor.abort_edit() {
                    Ok(()) => Err(error),
                    Err(abort_error) => Err(format!("{error}; abort failed: {abort_error}")),
                };
            }
        }
        self.apply_changes(editor, plan.changes.clone(), false)
    }

    fn apply_file_properties(
        &self,
        editor: &mut dyn CommitEditor,
        change: &PlannedChange,
        map_mode_properties: bool,
    ) -> Result<(), String> {
        if map_mode_properties {
            for (name, value) in self
                .property_mapper
                .file_properties(change.symlink, change.executable)
            {
                editor.change_file_prop(&change.path, &name, value.as_deref())?;
            }
        }
        for property in &change.properties {
            editor.change_file_prop(&change.path, &property.name, property.value.as_deref())?;
        }
        Ok(())
    }
}
