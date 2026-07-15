use git_svn_rs_core::dcommit::{
    DcommitPlan, DcommitTarget, PathEnsurer, PlannedChange, PropertyChange, PropertyMapper,
    SvnCommitEditor,
};
use git_svn_rs_core::svn::editor::CommitEditor;

#[derive(Default)]
struct RecordingEditor {
    calls: Vec<String>,
    fail_open: bool,
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
        if self.fail_open {
            Err("injected open failure".to_string())
        } else {
            Ok(())
        }
    }

    fn delete_entry(&mut self, path: &str) -> Result<(), String> {
        self.calls.push(format!("delete {path}"));
        Ok(())
    }

    fn copy_file(
        &mut self,
        source_path: &str,
        source_revision: u32,
        path: &str,
    ) -> Result<(), String> {
        self.calls
            .push(format!("copy {source_path}@{source_revision} {path}"));
        Ok(())
    }

    fn move_entry(
        &mut self,
        source_path: &str,
        source_revision: u32,
        path: &str,
    ) -> Result<(), String> {
        self.calls
            .push(format!("move {source_path}@{source_revision} {path}"));
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

    fn change_directory_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        self.calls.push(format!(
            "dir-prop {path} {name} {}",
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
fn commit_editor_applies_complete_plan_operations_and_properties() {
    let plan = DcommitPlan {
        target: DcommitTarget {
            url: "file:///repo/trunk".to_string(),
            repository_root: "file:///repo".to_string(),
            repository_uuid: "uuid".to_string(),
            git_ref: "refs/remotes/git-svn".to_string(),
        },
        base_revision: 6,
        git_commit: "deadbeef".to_string(),
        message: "message\nbody\n".to_string(),
        author: Some("A U Thor <author@example.com>".to_string()),
        root_properties: vec![PropertyChange::set("svn:mergeinfo", "/branches/a:1-6")],
        changes: vec![
            PlannedChange::copy_file("old.txt", 6, "nested/copied.txt", b"copied final\n")
                .with_property(PropertyChange::delete("svn:executable")),
            PlannedChange::move_entry("old-dir", 6, "nested/new-dir", b"moved final\n"),
        ],
    };
    let mut editor = RecordingEditor::default();

    let revision = SvnCommitEditor::new(PropertyMapper)
        .apply_plan(&mut editor, &plan)
        .unwrap();

    assert_eq!(revision, 7);
    assert_eq!(
        editor.calls,
        vec![
            "dir-prop  svn:mergeinfo /branches/a:1-6",
            "ensure nested",
            "copy old.txt@6 nested/copied.txt",
            "open nested/copied.txt copied final\n",
            "prop nested/copied.txt svn:executable <deleted>",
            "move old-dir@6 nested/new-dir",
            "open nested/new-dir moved final\n",
            "close",
        ]
    );
}

#[test]
fn commit_editor_aborts_once_when_an_operation_fails() {
    let mut editor = RecordingEditor {
        fail_open: true,
        ..RecordingEditor::default()
    };

    let error = SvnCommitEditor::new(PropertyMapper)
        .apply(
            &mut editor,
            [PlannedChange::copy_file(
                "old.txt", 6, "new.txt", b"final\n",
            )],
        )
        .unwrap_err();

    assert_eq!(error, "injected open failure");
    assert_eq!(
        editor.calls,
        vec!["copy old.txt@6 new.txt", "open new.txt final\n", "abort",]
    );
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
