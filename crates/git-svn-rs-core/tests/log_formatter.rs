use git_svn_rs_core::log_formatter::{GitSvnLogEntry, GitSvnLogFormatter};

#[test]
fn formats_svn_style_log_entry() {
    let entry = GitSvnLogEntry {
        revision: 7,
        author: "alice".to_string(),
        date: "2026-01-01T00:00:00Z".to_string(),
        message: "add file\n\nbody".to_string(),
        commit: "abcdef1234567890".to_string(),
        abbreviated_commit: None,
        changed_paths: vec!["M\ttrunk/src/lib.rs".to_string()],
    };

    let rendered = GitSvnLogFormatter::default().format_entry(&entry);

    assert!(
        rendered
            .contains("------------------------------------------------------------------------\n")
    );
    assert!(rendered.contains("r7 | abcdef1 | alice | 2026-01-01T00:00:00Z | 3 lines\n"));
    assert!(rendered.contains("Changed paths:\n   M\ttrunk/src/lib.rs\n"));
    assert!(!rendered.contains("\ncommit abcdef1234567890\n"));
    assert!(rendered.contains("add file\n\nbody\n"));
}

#[test]
fn normal_log_entry_without_show_commit_keeps_svn_header() {
    let entry = GitSvnLogEntry {
        revision: 8,
        author: "alice".to_string(),
        date: "2026-01-01T00:00:00Z".to_string(),
        message: "add file".to_string(),
        commit: "abcdef1234567890".to_string(),
        abbreviated_commit: None,
        changed_paths: Vec::new(),
    };

    let rendered = GitSvnLogFormatter::new(false, false, false).format_entry(&entry);

    assert!(rendered.contains("r8 | alice | 2026-01-01T00:00:00Z | 1 line\n"));
    assert!(!rendered.contains("abcdef1234567890"));
}

#[test]
fn formats_oneline_log_entry() {
    let entry = GitSvnLogEntry {
        revision: 12,
        author: "bob".to_string(),
        date: "2026-01-02T00:00:00Z".to_string(),
        message: "update file\n\nignored body".to_string(),
        commit: "1234567890abcdef".to_string(),
        abbreviated_commit: None,
        changed_paths: Vec::new(),
    };

    let rendered = GitSvnLogFormatter::oneline().format_entry(&entry);

    assert_eq!(rendered, "r12 | update file\n");
}

#[test]
fn oneline_log_entry_can_show_commit() {
    let entry = GitSvnLogEntry {
        revision: 14,
        author: "dana".to_string(),
        date: "2026-01-04T00:00:00Z".to_string(),
        message: "show commit".to_string(),
        commit: "abc123def456".to_string(),
        abbreviated_commit: None,
        changed_paths: Vec::new(),
    };

    let rendered = GitSvnLogFormatter::new(true, true, false).format_entry(&entry);

    assert_eq!(rendered, "r14 | abc123d | show commit\n");
}

#[test]
fn oneline_revision_is_right_padded_to_the_first_entry_width() {
    let entry = GitSvnLogEntry {
        revision: 9,
        author: "alice".to_string(),
        date: "2026-01-01 00:00:00 +0000 (Thu, 01 Jan 2026)".to_string(),
        message: "subject".to_string(),
        commit: "0123456789abcdef".to_string(),
        abbreviated_commit: None,
        changed_paths: Vec::new(),
    };

    assert_eq!(
        GitSvnLogFormatter::oneline().format_entry_with_revision_width(&entry, Some(2)),
        "r9  | subject\n"
    );
}

#[test]
fn incremental_log_entry_keeps_leading_separator() {
    let entry = GitSvnLogEntry {
        revision: 13,
        author: "carol".to_string(),
        date: "2026-01-03T00:00:00Z".to_string(),
        message: "incremental entry".to_string(),
        commit: "fedcba0987654321".to_string(),
        abbreviated_commit: None,
        changed_paths: Vec::new(),
    };

    let rendered = GitSvnLogFormatter::incremental(false, false).format_entry(&entry);

    assert!(
        rendered.starts_with(
            "------------------------------------------------------------------------\n"
        )
    );
    assert!(rendered.contains("r13 | carol | 2026-01-03T00:00:00Z | 1 line\n"));
    assert!(rendered.contains("incremental entry\n"));
}

#[test]
fn incremental_log_entry_can_show_short_commit_without_trailing_separator() {
    let entry = GitSvnLogEntry {
        revision: 17,
        author: "grace".to_string(),
        date: "2026-01-07T00:00:00Z".to_string(),
        message: "incremental commit".to_string(),
        commit: "abcdef1234567890".to_string(),
        abbreviated_commit: None,
        changed_paths: Vec::new(),
    };

    let rendered = GitSvnLogFormatter::incremental(true, false).format_entry(&entry);

    assert_eq!(
        rendered,
        "------------------------------------------------------------------------\nr17 | abcdef1 | grace | 2026-01-07T00:00:00Z | 1 line\n\nincremental commit\n"
    );
}

#[test]
fn counts_internal_blank_lines_and_empty_messages() {
    let entry_with_blank_line = GitSvnLogEntry {
        revision: 15,
        author: "erin".to_string(),
        date: "2026-01-05T00:00:00Z".to_string(),
        message: "subject\n\nbody".to_string(),
        commit: "0123456789abcdef".to_string(),
        abbreviated_commit: None,
        changed_paths: Vec::new(),
    };
    let empty_entry = GitSvnLogEntry {
        revision: 16,
        author: "frank".to_string(),
        date: "2026-01-06T00:00:00Z".to_string(),
        message: String::new(),
        commit: "0123456789abcdef".to_string(),
        abbreviated_commit: None,
        changed_paths: Vec::new(),
    };

    let formatter = GitSvnLogFormatter::new(false, false, false);

    assert!(
        formatter
            .format_entry(&entry_with_blank_line)
            .contains("r15 | erin | 2026-01-05T00:00:00Z | 3 lines\n")
    );
    assert!(
        formatter
            .format_entry(&empty_entry)
            .contains("r16 | frank | 2026-01-06T00:00:00Z | 1 line\n")
    );
}

#[test]
fn oneline_skips_leading_blank_lines_like_frozen_log_pm() {
    let entry = GitSvnLogEntry {
        revision: 18,
        author: "alice".to_string(),
        date: "2026-01-01 00:00:00 +0000 (Thu, 01 Jan 2026)".to_string(),
        message: "\n\nsubject\nbody".to_string(),
        commit: "0123456789abcdef".to_string(),
        abbreviated_commit: None,
        changed_paths: Vec::new(),
    };

    assert_eq!(
        GitSvnLogFormatter::oneline().format_entry(&entry),
        "r18 | subject\n"
    );
}

#[test]
fn preserves_trailing_message_whitespace_and_counts_a_trailing_blank_line() {
    let entry = GitSvnLogEntry {
        revision: 19,
        author: "alice".to_string(),
        date: "2026-01-01 00:00:00 +0000 (Thu, 01 Jan 2026)".to_string(),
        message: "subject  \n".to_string(),
        commit: "0123456789abcdef".to_string(),
        abbreviated_commit: None,
        changed_paths: Vec::new(),
    };

    let rendered = GitSvnLogFormatter::new(false, false, false).format_entry(&entry);
    assert!(rendered.contains(" | 2 lines\n\nsubject  \n"));
}

#[test]
fn normal_log_preserves_passthrough_git_output_after_the_message() {
    let entry = GitSvnLogEntry {
        revision: 20,
        author: "alice".to_string(),
        date: "2026-01-01 00:00:00 +0000 (Thu, 01 Jan 2026)".to_string(),
        message: "subject".to_string(),
        commit: "0123456789abcdef".to_string(),
        abbreviated_commit: None,
        changed_paths: Vec::new(),
    };

    let rendered = GitSvnLogFormatter::new(false, false, false).format_entry_with_git_output(
        &entry,
        None,
        "\n file.txt | 1 +\n",
    );

    assert!(rendered.contains("\nsubject\n\n file.txt | 1 +\n"));
}
