use std::path::{Path, PathBuf};

use crate::dcommit::diff_planner::normalize_commit_path;
use crate::svn::editor::CommitEditor;

use super::{DcommitSvnOptions, run_svn, svn_commit};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorState {
    Open,
    Closed,
    Aborted,
}

/// Applies a typed dcommit plan to an SVN CLI working copy.
pub(super) struct WorkingCopyPlanEditor<'a> {
    wc: PathBuf,
    svn_options: &'a DcommitSvnOptions,
    message: String,
    expected_base: u32,
    state: EditorState,
}

impl<'a> WorkingCopyPlanEditor<'a> {
    pub(super) fn new(
        wc: impl Into<PathBuf>,
        svn_options: &'a DcommitSvnOptions,
        message: impl Into<String>,
        expected_base: u32,
    ) -> Self {
        Self {
            wc: wc.into(),
            svn_options,
            message: message.into(),
            expected_base,
            state: EditorState::Open,
        }
    }

    fn require_open(&self) -> Result<(), String> {
        match self.state {
            EditorState::Open => Ok(()),
            EditorState::Closed => Err("working-copy commit editor is already closed".to_string()),
            EditorState::Aborted => Err("working-copy commit editor was aborted".to_string()),
        }
    }

    fn path(&self, path: &str) -> Result<(String, PathBuf), String> {
        self.require_open()?;
        let path = normalize_commit_path(path)?;
        Ok((path.clone(), self.wc.join(path)))
    }

    fn property_target(&self, path: &str) -> Result<String, String> {
        self.require_open()?;
        if path.is_empty() {
            Ok(".".to_string())
        } else {
            normalize_commit_path(path)
        }
    }

    fn change_prop(&self, path: &str, name: &str, value: Option<&str>) -> Result<(), String> {
        let target = self.property_target(path)?;
        let mut args = match value {
            Some(value) => vec![
                "propset".to_string(),
                "--non-interactive".to_string(),
                name.to_string(),
                value.to_string(),
            ],
            None => vec![
                "propdel".to_string(),
                "--non-interactive".to_string(),
                name.to_string(),
            ],
        };
        args.push(target);
        run_svn(Some(&self.wc), self.svn_options, &args)
    }

    fn verify_copy_source(&self, source_path: &str, revision: u32) -> Result<String, String> {
        self.require_open()?;
        if revision != self.expected_base {
            return Err(format!(
                "copy source revision r{revision} does not match working-copy base r{}",
                self.expected_base
            ));
        }
        normalize_commit_path(source_path)
    }

    fn write_file(&self, path: &Path, content: &[u8]) -> Result<(), String> {
        std::fs::write(path, content)
            .map_err(|error| format!("failed to write {}: {error}", path.display()))
    }
}

impl CommitEditor for WorkingCopyPlanEditor<'_> {
    fn ensure_path(&mut self, path: &str) -> Result<(), String> {
        let (path, target) = self.path(path)?;
        if target.exists() {
            return Ok(());
        }
        std::fs::create_dir_all(&target)
            .map_err(|error| format!("failed to create {}: {error}", target.display()))?;
        run_svn(
            Some(&self.wc),
            self.svn_options,
            &["add".to_string(), "--parents".to_string(), path],
        )
    }

    fn add_file(&mut self, path: &str, content: &[u8]) -> Result<(), String> {
        let (path, target) = self.path(path)?;
        self.write_file(&target, content)?;
        run_svn(
            Some(&self.wc),
            self.svn_options,
            &["add".to_string(), "--parents".to_string(), path],
        )
    }

    fn open_file(&mut self, path: &str, content: &[u8]) -> Result<(), String> {
        let (_, target) = self.path(path)?;
        self.write_file(&target, content)
    }

    fn delete_entry(&mut self, path: &str) -> Result<(), String> {
        let (path, _) = self.path(path)?;
        run_svn(
            Some(&self.wc),
            self.svn_options,
            &["delete".to_string(), path],
        )
    }

    fn copy_file(
        &mut self,
        source_path: &str,
        source_revision: u32,
        path: &str,
    ) -> Result<(), String> {
        let source_path = self.verify_copy_source(source_path, source_revision)?;
        let (path, _) = self.path(path)?;
        run_svn(
            Some(&self.wc),
            self.svn_options,
            &["copy".to_string(), source_path, path],
        )
    }

    fn move_entry(
        &mut self,
        source_path: &str,
        source_revision: u32,
        path: &str,
    ) -> Result<(), String> {
        let source_path = self.verify_copy_source(source_path, source_revision)?;
        let (path, _) = self.path(path)?;
        run_svn(
            Some(&self.wc),
            self.svn_options,
            &["move".to_string(), source_path, path],
        )
    }

    fn change_file_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        self.change_prop(path, name, value)
    }

    fn change_directory_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        self.change_prop(path, name, value)
    }

    fn close_edit(&mut self) -> Result<u32, String> {
        self.require_open()?;
        let revision = svn_commit(&self.wc, &self.message, self.svn_options)?;
        self.state = EditorState::Closed;
        Ok(revision)
    }

    fn abort_edit(&mut self) -> Result<(), String> {
        self.require_open()?;
        self.state = EditorState::Aborted;
        run_svn(
            Some(&self.wc),
            self.svn_options,
            &[
                "revert".to_string(),
                "--recursive".to_string(),
                ".".to_string(),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_paths_outside_the_working_copy_before_running_svn() {
        let options = DcommitSvnOptions::default();
        let mut editor = WorkingCopyPlanEditor::new("missing", &options, "message", 7);

        let error = editor.add_file("../escape", b"content").unwrap_err();

        assert!(error.contains("outside commit root"), "{error}");
    }

    #[test]
    fn rejects_copy_revision_other_than_the_checked_out_base() {
        let options = DcommitSvnOptions::default();
        let mut editor = WorkingCopyPlanEditor::new("missing", &options, "message", 7);

        let error = editor.copy_file("old.txt", 6, "new.txt").unwrap_err();

        assert_eq!(
            error,
            "copy source revision r6 does not match working-copy base r7"
        );
    }

    #[test]
    fn closed_and_aborted_states_are_terminal() {
        let options = DcommitSvnOptions::default();
        let mut closed = WorkingCopyPlanEditor::new("missing", &options, "message", 7);
        closed.state = EditorState::Closed;
        assert_eq!(
            closed.close_edit().unwrap_err(),
            "working-copy commit editor is already closed"
        );

        let mut aborted = WorkingCopyPlanEditor::new("missing", &options, "message", 7);
        aborted.state = EditorState::Aborted;
        assert_eq!(
            aborted.close_edit().unwrap_err(),
            "working-copy commit editor was aborted"
        );
    }
}
