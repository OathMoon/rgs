use git_svn_rs_core::fast_import::FileChange;
use git_svn_rs_core::fetch_editor::{FetchCommitPlan, SvnFetchEditor, TreeEntry};
use git_svn_rs_core::svn::editor::FetchEditor;

fn plan() -> FetchCommitPlan {
    FetchCommitPlan {
        mark: 3,
        refname: "refs/remotes/git-svn".to_string(),
        author: "Ada <ada@example.com>".to_string(),
        committer: "Ada <ada@example.com>".to_string(),
        timestamp: 1_704_067_200,
        message: "import r3".to_string(),
        parent_mark: Some(2),
    }
}

#[test]
fn added_file_with_textdelta_becomes_fast_import_modify() {
    let mut editor = SvnFetchEditor::new(plan());

    editor.open_root(3).unwrap();
    editor.add_directory("trunk", None).unwrap();
    editor.add_file("trunk/src/main.rs", None).unwrap();
    editor
        .apply_textdelta("trunk/src/main.rs", b"fn main() {}\n")
        .unwrap();
    editor.close_edit().unwrap();

    let commit = editor.into_commit().unwrap();

    assert_eq!(commit.mark, 3);
    assert_eq!(commit.parent_mark, Some(2));
    assert_eq!(
        commit.changes,
        vec![FileChange::Modify {
            path: "trunk/src/main.rs".to_string(),
            mode: "100644".to_string(),
            content: b"fn main() {}\n".to_vec(),
        }]
    );
}

#[test]
fn configured_path_prefix_is_stripped_from_fast_import_paths() {
    let mut editor = SvnFetchEditor::new(plan()).with_path_prefix("trunk");

    editor.open_root(3).unwrap();
    editor.add_directory("trunk/src", None).unwrap();
    editor.add_file("trunk/src/main.rs", None).unwrap();
    editor
        .apply_textdelta("trunk/src/main.rs", b"fn main() {}\n")
        .unwrap();
    editor.close_edit().unwrap();

    let commit = editor.into_commit().unwrap();

    assert_eq!(
        commit.changes,
        vec![FileChange::Modify {
            path: "src/main.rs".to_string(),
            mode: "100644".to_string(),
            content: b"fn main() {}\n".to_vec(),
        }]
    );
}

#[test]
fn executable_property_changes_file_mode() {
    let mut editor = SvnFetchEditor::new(plan());

    editor.open_root(3).unwrap();
    editor.add_file("trunk/script.sh", None).unwrap();
    editor
        .change_file_prop("trunk/script.sh", "svn:executable", Some("*"))
        .unwrap();
    editor
        .apply_textdelta("trunk/script.sh", b"#!/bin/sh\ntrue\n")
        .unwrap();
    editor.close_edit().unwrap();

    let commit = editor.into_commit().unwrap();

    assert_eq!(
        commit.changes,
        vec![FileChange::Modify {
            path: "trunk/script.sh".to_string(),
            mode: "100755".to_string(),
            content: b"#!/bin/sh\ntrue\n".to_vec(),
        }]
    );
}

#[test]
fn special_property_changes_file_to_symlink() {
    let mut editor = SvnFetchEditor::new(plan());

    editor.open_root(3).unwrap();
    editor.add_file("trunk/link", None).unwrap();
    editor
        .change_file_prop("trunk/link", "svn:special", Some("*"))
        .unwrap();
    editor
        .apply_textdelta("trunk/link", b"link target.txt")
        .unwrap();
    editor.close_edit().unwrap();

    let commit = editor.into_commit().unwrap();

    assert_eq!(
        commit.changes,
        vec![FileChange::Modify {
            path: "trunk/link".to_string(),
            mode: "120000".to_string(),
            content: b"target.txt".to_vec(),
        }]
    );
}

#[test]
fn copied_directory_materializes_existing_subtree_at_destination() {
    let mut editor = SvnFetchEditor::with_base_tree(
        plan(),
        vec![
            TreeEntry::file("trunk/README.md", "100644", b"hello\n"),
            TreeEntry::file("trunk/bin/run", "100755", b"#!/bin/sh\n"),
        ],
    );

    editor.open_root(3).unwrap();
    editor
        .add_directory("branches/topic", Some(("trunk", 2)))
        .unwrap();
    editor.close_edit().unwrap();

    let commit = editor.into_commit().unwrap();

    assert_eq!(
        commit.changes,
        vec![
            FileChange::Modify {
                path: "branches/topic/README.md".to_string(),
                mode: "100644".to_string(),
                content: b"hello\n".to_vec(),
            },
            FileChange::Modify {
                path: "branches/topic/bin/run".to_string(),
                mode: "100755".to_string(),
                content: b"#!/bin/sh\n".to_vec(),
            },
        ]
    );
}

#[test]
fn delete_entry_emits_delete_and_drops_pending_child_changes() {
    let mut editor = SvnFetchEditor::new(plan());

    editor.open_root(3).unwrap();
    editor.add_file("trunk/obsolete.txt", None).unwrap();
    editor
        .apply_textdelta("trunk/obsolete.txt", b"temporary\n")
        .unwrap();
    editor.delete_entry("trunk", 2).unwrap();
    editor.close_edit().unwrap();

    let commit = editor.into_commit().unwrap();

    assert_eq!(
        commit.changes,
        vec![FileChange::Delete {
            path: "trunk".to_string(),
        }]
    );
}
