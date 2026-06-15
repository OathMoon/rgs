use git_svn_rs_core::svn::editor::CommitEditor;
use git_svn_rs_core::svn::mock::MockSvnBackend;
use git_svn_rs_core::svn::{CommitRecord, RevisionEvent, SvnBackend};

#[test]
fn mock_backend_filters_revision_window() {
    let backend = MockSvnBackend::new(
        "uuid",
        vec![
            RevisionEvent {
                revision: 1,
                author: "alice".to_string(),
                message: "one".to_string(),
                timestamp: "2026-01-01T00:00:00Z".to_string(),
                changed_paths: vec![],
            },
            RevisionEvent {
                revision: 2,
                author: "bob".to_string(),
                message: "two".to_string(),
                timestamp: "2026-01-02T00:00:00Z".to_string(),
                changed_paths: vec![],
            },
        ],
    );

    assert_eq!(backend.uuid().unwrap(), "uuid");
    assert_eq!(backend.latest_revnum().unwrap(), 2);
    assert_eq!(backend.log(2, 2).unwrap()[0].author, "bob");
}

#[test]
fn mock_commit_editor_records_operations_and_returns_new_revision() {
    let mut backend = MockSvnBackend::new("uuid", vec![]);
    let mut editor = backend.commit_editor(CommitRecord {
        author: "alice".to_string(),
        message: "dcommit".to_string(),
        base_revision: 3,
    });

    editor.ensure_path("trunk/src").unwrap();
    editor
        .add_file("trunk/src/lib.rs", b"pub fn answer() -> u8 { 42 }\n")
        .unwrap();
    editor
        .change_file_prop("trunk/src/lib.rs", "svn:eol-style", Some("LF"))
        .unwrap();
    let revision = editor.close_edit().unwrap();

    assert_eq!(revision, 4);
    let commits = backend.commits();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].record.author, "alice");
    assert_eq!(
        commits[0].operations,
        vec![
            "ensure trunk/src",
            "add trunk/src/lib.rs pub fn answer() -> u8 { 42 }\n",
            "prop trunk/src/lib.rs svn:eol-style LF",
        ]
    );
}
