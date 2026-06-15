use git_svn_rs_core::dcommit::{GitDiffChange, GitDiffPlanner, PlannedChange, PlannedChangeKind};

#[test]
fn planner_orders_directory_creation_before_file_adds() {
    let plan = GitDiffPlanner::new()
        .plan([
            GitDiffChange::add_file("src/lib.rs", b"pub fn answer() -> u8 { 42 }\n"),
            GitDiffChange::modify_file("README.md", b"# Project\n"),
        ])
        .unwrap();

    assert_eq!(
        plan.changes,
        vec![
            PlannedChange::ensure_dir("src"),
            PlannedChange::add_file("src/lib.rs", b"pub fn answer() -> u8 { 42 }\n"),
            PlannedChange::modify_file("README.md", b"# Project\n"),
        ]
    );
}

#[test]
fn planner_deletes_children_before_parents() {
    let plan = GitDiffPlanner::new()
        .plan([
            GitDiffChange::delete("src"),
            GitDiffChange::delete("src/lib.rs"),
            GitDiffChange::delete("src/nested/mod.rs"),
        ])
        .unwrap();

    let paths: Vec<_> = plan
        .changes
        .iter()
        .map(|change| (change.path.as_str(), change.kind.clone()))
        .collect();

    assert_eq!(
        paths,
        vec![
            ("src/nested/mod.rs", PlannedChangeKind::Delete),
            ("src/lib.rs", PlannedChangeKind::Delete),
            ("src", PlannedChangeKind::Delete),
        ]
    );
}

#[test]
fn planner_rejects_paths_that_escape_the_commit_root() {
    let err = GitDiffPlanner::new()
        .plan([GitDiffChange::add_file("../outside.txt", b"nope")])
        .unwrap_err();

    assert!(err.contains("outside commit root"));
}
