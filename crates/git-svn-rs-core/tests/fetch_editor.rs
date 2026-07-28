use git_svn_rs_core::fast_import::FileChange;
use git_svn_rs_core::fetch_editor::{FetchCommitPlan, SvnFetchEditor, TreeEntry};
use git_svn_rs_core::git::{GitCli, GitTreeFile};
use git_svn_rs_core::svn::editor::FetchEditor;

fn plan() -> FetchCommitPlan {
    FetchCommitPlan {
        mark: 3,
        refname: "refs/remotes/git-svn".to_string(),
        author: "Ada <ada@example.com>".to_string(),
        committer: "Ada <ada@example.com>".to_string(),
        timestamp: 1_704_067_200,
        timezone_offset: "+0000".to_string(),
        message: "import r3".to_string(),
        parent_mark: Some(2),
        parent_ref: None,
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
fn commit_plan_can_carry_parent_ref_for_incremental_editor_import() {
    let mut plan = plan();
    plan.parent_mark = None;
    plan.parent_ref = Some("refs/remotes/origin/trunk".to_string());
    let mut editor = SvnFetchEditor::new(plan);

    editor.open_root(4).unwrap();
    editor.add_file("trunk/new.rs", None).unwrap();
    editor
        .apply_textdelta("trunk/new.rs", b"pub fn new() {}\n")
        .unwrap();
    editor.close_edit().unwrap();

    let commit = editor.into_commit().unwrap();

    assert_eq!(commit.parent_mark, None);
    assert_eq!(
        commit.parent_ref.as_deref(),
        Some("refs/remotes/origin/trunk")
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
fn replacing_a_symlink_with_a_regular_file_resets_base_mode() {
    let mut editor = SvnFetchEditor::with_base_tree(
        plan(),
        vec![TreeEntry::file("trunk/link", "120000", b"old-target")],
    );

    editor.open_root(3).unwrap();
    editor.delete_entry("trunk/link", 2).unwrap();
    editor.add_file("trunk/link", None).unwrap();
    editor.apply_textdelta("trunk/link", b"regular\n").unwrap();
    editor.close_edit().unwrap();

    assert_eq!(
        editor.into_commit().unwrap().changes,
        vec![FileChange::Modify {
            path: "trunk/link".to_string(),
            mode: "100644".to_string(),
            content: b"regular\n".to_vec(),
        }]
    );
}

#[test]
fn real_child_removes_an_owned_empty_directory_placeholder() {
    let mut editor = SvnFetchEditor::with_base_tree(
        plan(),
        vec![TreeEntry::file("empty/.gitkeep", "100644", b"")],
    )
    .with_owned_placeholders(["empty".to_string()]);

    editor.open_root(3).unwrap();
    editor.add_file("empty/value.txt", None).unwrap();
    editor
        .apply_textdelta("empty/value.txt", b"value\n")
        .unwrap();
    editor.reconcile_empty_directories(".gitkeep").unwrap();
    editor.close_edit().unwrap();

    let result = editor.into_result().unwrap();
    assert_eq!(
        result.commit.changes,
        vec![
            FileChange::Delete {
                path: "empty/.gitkeep".to_string(),
            },
            FileChange::Modify {
                path: "empty/value.txt".to_string(),
                mode: "100644".to_string(),
                content: b"value\n".to_vec(),
            },
        ]
    );
    assert_eq!(
        result.unhandled.lines(),
        vec!["  -empty_dir: empty".to_string()]
    );
    assert!(result.owned_placeholders.is_empty());
}

#[test]
fn real_same_named_placeholder_is_preserved_when_a_sibling_is_added() {
    let mut editor = SvnFetchEditor::with_base_tree(
        plan(),
        vec![TreeEntry::file(
            "directory/.gitkeep",
            "100644",
            b"repository content\n",
        )],
    );

    editor.open_root(3).unwrap();
    editor.add_file("directory/value.txt", None).unwrap();
    editor
        .apply_textdelta("directory/value.txt", b"value\n")
        .unwrap();
    editor.reconcile_empty_directories(".gitkeep").unwrap();
    editor.close_edit().unwrap();

    let result = editor.into_result().unwrap();
    assert_eq!(
        result.commit.changes,
        vec![FileChange::Modify {
            path: "directory/value.txt".to_string(),
            mode: "100644".to_string(),
            content: b"value\n".to_vec(),
        }]
    );
    assert!(result.unhandled.is_empty());
    assert!(result.owned_placeholders.is_empty());
}

#[test]
fn deleting_the_last_real_child_creates_a_persistent_placeholder() {
    let mut editor = SvnFetchEditor::with_base_tree(
        plan(),
        vec![TreeEntry::file("directory/value.txt", "100644", b"value\n")],
    );

    editor.open_root(3).unwrap();
    editor.delete_entry("directory/value.txt", 2).unwrap();
    editor.reconcile_empty_directories(".gitkeep").unwrap();
    editor.close_edit().unwrap();

    let result = editor.into_result().unwrap();
    assert_eq!(
        result.commit.changes,
        vec![
            FileChange::Modify {
                path: "directory/.gitkeep".to_string(),
                mode: "100644".to_string(),
                content: Vec::new(),
            },
            FileChange::Delete {
                path: "directory/value.txt".to_string(),
            },
        ]
    );
    assert_eq!(
        result.unhandled.lines(),
        vec!["  +empty_dir: directory".to_string()]
    );
    assert_eq!(
        result.owned_placeholders,
        ["directory".to_string()].into_iter().collect()
    );
}

#[test]
fn nested_empty_directories_need_only_the_deepest_placeholder() {
    let mut editor = SvnFetchEditor::new(plan());

    editor.open_root(3).unwrap();
    editor.add_directory("outer", None).unwrap();
    editor.add_directory("outer/inner", None).unwrap();
    editor.reconcile_empty_directories(".gitkeep").unwrap();
    editor.close_edit().unwrap();

    let result = editor.into_result().unwrap();
    assert_eq!(
        result.commit.changes,
        vec![FileChange::Modify {
            path: "outer/inner/.gitkeep".to_string(),
            mode: "100644".to_string(),
            content: Vec::new(),
        }]
    );
    assert_eq!(
        result.unhandled.lines(),
        vec!["  +empty_dir: outer/inner".to_string()]
    );
}

#[test]
fn unhandled_metadata_matches_git_svn_log_format() {
    let mut editor = SvnFetchEditor::new(plan());

    editor.open_root(3).unwrap();
    editor
        .change_directory_prop("", "custom:root prop", Some("root value"))
        .unwrap();
    editor.add_file("trunk/file name", None).unwrap();
    editor
        .change_file_prop("trunk/file name", "custom:file", Some("line 1\nline 2"))
        .unwrap();
    editor
        .change_file_prop("trunk/file name", "svn:entry:uuid", Some("ignored"))
        .unwrap();
    editor
        .change_file_prop("trunk/file name", "custom:removed", None)
        .unwrap();
    editor.absent_file("trunk/private file").unwrap();
    editor.absent_directory("trunk/private directory").unwrap();
    editor.close_edit().unwrap();

    let result = editor.into_result().unwrap();

    assert_eq!(
        result.unhandled.lines(),
        vec![
            "  +dir_prop: . custom:root%20prop root%20value",
            "  +file_prop: trunk/file%20name custom:file line%201%0Aline%202",
            "  -file_prop: trunk/file%20name custom:removed",
            "  absent_file: trunk/private%20file",
            "  absent_directory: trunk/private%20directory",
        ]
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
fn copied_directory_transfers_placeholder_ownership_to_the_destination() {
    let mut editor = SvnFetchEditor::with_base_tree(
        plan(),
        vec![TreeEntry::file("trunk/empty/.gitkeep", "100644", b"")],
    )
    .with_owned_placeholders(["trunk/empty".to_string()]);

    editor.open_root(3).unwrap();
    editor
        .add_directory("branches/topic", Some(("trunk", 2)))
        .unwrap();
    editor.reconcile_empty_directories(".gitkeep").unwrap();
    editor.close_edit().unwrap();

    let result = editor.into_result().unwrap();
    assert!(result.commit.changes.contains(&FileChange::Modify {
        path: "branches/topic/empty/.gitkeep".to_string(),
        mode: "100644".to_string(),
        content: Vec::new(),
    }));
    assert!(result.owned_placeholders.contains("branches/topic/empty"));
    assert_eq!(
        result.unhandled.lines(),
        vec!["  +empty_dir: branches/topic/empty".to_string()]
    );
}

#[test]
fn tree_entry_can_be_built_from_git_tree_file() {
    let entry = TreeEntry::from_git_file(GitTreeFile {
        path: "trunk/bin/run".to_string(),
        mode: "100755".to_string(),
        content: b"#!/bin/sh\n".to_vec(),
    });
    let mut editor = SvnFetchEditor::with_base_tree(plan(), vec![entry]);

    editor.open_root(3).unwrap();
    editor
        .add_file("branches/topic/run", Some(("trunk/bin/run", 2)))
        .unwrap();
    editor.close_edit().unwrap();

    let commit = editor.into_commit().unwrap();

    assert_eq!(
        commit.changes,
        vec![FileChange::Modify {
            path: "branches/topic/run".to_string(),
            mode: "100755".to_string(),
            content: b"#!/bin/sh\n".to_vec(),
        }]
    );
}

#[test]
fn editor_can_load_base_tree_from_git_ref() {
    let dir = tempfile::tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    git.run_for_test(["config", "user.name", "Test User"])
        .unwrap();
    git.run_for_test(["config", "user.email", "test@example.com"])
        .unwrap();
    std::fs::create_dir_all(dir.path().join("trunk/src")).unwrap();
    std::fs::write(dir.path().join("trunk/src/lib.rs"), "pub fn old() {}\n").unwrap();
    git.run_for_test(["add", "trunk/src/lib.rs"]).unwrap();
    git.run_for_test(["commit", "-m", "base"]).unwrap();
    git.run_for_test(["update-ref", "refs/remotes/origin/trunk", "HEAD"])
        .unwrap();
    let mut editor =
        SvnFetchEditor::from_git_ref(&git, plan(), "refs/remotes/origin/trunk").unwrap();

    editor.open_root(3).unwrap();
    editor
        .add_file("branches/topic/src/lib.rs", Some(("trunk/src/lib.rs", 2)))
        .unwrap();
    editor.close_edit().unwrap();

    let commit = editor.into_commit().unwrap();

    assert_eq!(
        commit.changes,
        vec![FileChange::Modify {
            path: "branches/topic/src/lib.rs".to_string(),
            mode: "100644".to_string(),
            content: b"pub fn old() {}\n".to_vec(),
        }]
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

#[test]
fn deleting_mapped_root_deletes_base_files_without_an_empty_git_path() {
    let mut editor = SvnFetchEditor::with_base_tree(
        plan(),
        vec![TreeEntry::file("old.txt", "100644", b"old\n")],
    )
    .with_path_prefix("branches/topic");

    editor.open_root(3).unwrap();
    editor.delete_entry("branches/topic", 2).unwrap();
    editor.close_edit().unwrap();

    assert_eq!(
        editor.into_commit().unwrap().changes,
        vec![FileChange::Delete {
            path: "old.txt".to_string(),
        }]
    );
}
