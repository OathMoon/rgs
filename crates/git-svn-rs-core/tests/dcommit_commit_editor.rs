use git_svn_rs_core::dcommit::{PathEnsurer, PlannedChange, PropertyMapper, SvnCommitEditor};
use git_svn_rs_core::svn::editor::CommitEditor;

#[derive(Default)]
struct RecordingEditor {
    calls: Vec<String>,
}

impl CommitEditor for RecordingEditor {
    fn ensure_path(&mut self, path: &str) -> Result<(), String> {
        self.calls.push(format!("ensure {path}"));
        Ok(())
    }

    fn add_file(&mut self, path: &str, content: &[u8]) -> Result<(), String> {
        self.calls
            .push(format!("add {path} {}", String::from_utf8_lossy(content)));
        Ok(())
    }

    fn open_file(&mut self, path: &str, content: &[u8]) -> Result<(), String> {
        self.calls
            .push(format!("open {path} {}", String::from_utf8_lossy(content)));
        Ok(())
    }

    fn delete_entry(&mut self, path: &str) -> Result<(), String> {
        self.calls.push(format!("delete {path}"));
        Ok(())
    }

    fn change_file_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        self.calls.push(format!(
            "prop {path} {name} {}",
            value.unwrap_or("<deleted>")
        ));
        Ok(())
    }

    fn close_edit(&mut self) -> Result<u32, String> {
        self.calls.push("close".to_string());
        Ok(7)
    }

    fn abort_edit(&mut self) -> Result<(), String> {
        self.calls.push("abort".to_string());
        Ok(())
    }
}

#[test]
fn path_ensurer_emits_each_parent_once_in_parent_first_order() {
    let mut editor = RecordingEditor::default();
    let mut ensurer = PathEnsurer::default();

    ensurer
        .ensure_parent_dirs(&mut editor, "src/nested/lib.rs")
        .unwrap();
    ensurer
        .ensure_parent_dirs(&mut editor, "src/nested/mod.rs")
        .unwrap();
    ensurer
        .ensure_parent_dirs(&mut editor, "README.md")
        .unwrap();

    assert_eq!(editor.calls, vec!["ensure src", "ensure src/nested"]);
}

#[test]
fn property_mapper_maps_executable_and_symlink_flags() {
    let mapper = PropertyMapper;

    assert_eq!(
        mapper.file_properties(false, true),
        vec![("svn:executable".to_string(), Some("*".to_string()))]
    );
    assert_eq!(
        mapper.file_properties(true, false),
        vec![("svn:special".to_string(), Some("*".to_string()))]
    );
}

#[test]
fn commit_editor_drives_planned_changes_and_closes_to_revision() {
    let mut editor = RecordingEditor::default();
    let revision = SvnCommitEditor::new(PropertyMapper)
        .apply(
            &mut editor,
            [
                PlannedChange::ensure_dir("src"),
                PlannedChange::add_file("src/run.sh", b"#!/bin/sh\n").with_executable(true),
                PlannedChange::modify_file("README.md", b"# Project\n"),
                PlannedChange::delete("old.txt"),
            ],
        )
        .unwrap();

    assert_eq!(revision, 7);
    assert_eq!(
        editor.calls,
        vec![
            "ensure src",
            "add src/run.sh #!/bin/sh\n",
            "prop src/run.sh svn:executable *",
            "open README.md # Project\n",
            "delete old.txt",
            "close",
        ]
    );
}
