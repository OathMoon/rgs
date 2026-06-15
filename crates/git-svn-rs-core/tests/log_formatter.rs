use git_svn_rs_core::log_formatter::{GitSvnLogEntry, GitSvnLogFormatter};

#[test]
fn formats_svn_style_log_entry() {
    let entry = GitSvnLogEntry {
        revision: 7,
        author: "alice".to_string(),
        date: "2026-01-01T00:00:00Z".to_string(),
        message: "add file\n\nbody".to_string(),
        commit: "abcdef1234567890".to_string(),
        changed_paths: vec!["M\ttrunk/src/lib.rs".to_string()],
    };

    let rendered = GitSvnLogFormatter::default().format_entry(&entry);

    assert!(
        rendered
            .contains("------------------------------------------------------------------------\n")
    );
    assert!(rendered.contains("r7 | alice | 2026-01-01T00:00:00Z | 2 lines\n"));
    assert!(rendered.contains("Changed paths:\n   M\ttrunk/src/lib.rs\n"));
    assert!(rendered.contains("commit abcdef1234567890\n"));
    assert!(rendered.contains("add file\n\nbody\n"));
}

#[test]
fn formats_oneline_log_entry() {
    let entry = GitSvnLogEntry {
        revision: 12,
        author: "bob".to_string(),
        date: "2026-01-02T00:00:00Z".to_string(),
        message: "update file\n\nignored body".to_string(),
        commit: "1234567890abcdef".to_string(),
        changed_paths: Vec::new(),
    };

    let rendered = GitSvnLogFormatter::oneline().format_entry(&entry);

    assert_eq!(rendered, "r12 | update file\n");
}
