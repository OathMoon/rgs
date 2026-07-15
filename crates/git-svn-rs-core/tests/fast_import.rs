use git_svn_rs_core::fast_import::{FastImportCommit, FastImportStream, FileChange};

#[test]
fn serializes_single_file_commit() {
    let commit = FastImportCommit {
        mark: 1,
        refname: "refs/remotes/git-svn".to_string(),
        author: "Jane Doe <jane@example.com>".to_string(),
        committer: "Jane Doe <jane@example.com>".to_string(),
        timestamp: 1_704_067_200,
        timezone_offset: "+0530".to_string(),
        message: "add file\n\ngit-svn-id: file:///repo/trunk@2 uuid".to_string(),
        parent_mark: None,
        parent_ref: None,
        changes: vec![FileChange::Modify {
            path: "src/lib.rs".to_string(),
            mode: "100644".to_string(),
            content: b"pub fn answer() -> u8 { 42 }\n".to_vec(),
        }],
    };

    let stream = String::from_utf8(FastImportStream::new().commit(&commit).finish()).unwrap();

    assert!(stream.contains("commit refs/remotes/git-svn\n"));
    assert!(stream.contains("mark :1\n"));
    assert!(stream.contains("author Jane Doe <jane@example.com> 1704067200 +0530\n"));
    assert!(stream.contains("committer Jane Doe <jane@example.com> 1704067200 +0530\n"));
    assert!(stream.contains("M 100644 inline src/lib.rs\n"));
    assert!(stream.contains("data 29\npub fn answer() -> u8 { 42 }\n"));
}

#[test]
fn serializes_delete_and_symlink_modes() {
    let commit = FastImportCommit {
        mark: 2,
        refname: "refs/remotes/git-svn".to_string(),
        author: "A <a@example.com>".to_string(),
        committer: "A <a@example.com>".to_string(),
        timestamp: 1,
        timezone_offset: "+0000".to_string(),
        message: "change".to_string(),
        parent_mark: Some(1),
        parent_ref: None,
        changes: vec![
            FileChange::Delete {
                path: "old.txt".to_string(),
            },
            FileChange::Modify {
                path: "link".to_string(),
                mode: "120000".to_string(),
                content: b"target.txt".to_vec(),
            },
        ],
    };

    let stream = String::from_utf8(FastImportStream::new().commit(&commit).finish()).unwrap();

    assert!(stream.contains("from :1\n"));
    assert!(stream.contains("D old.txt\n"));
    assert!(stream.contains("M 120000 inline link\n"));
}

#[test]
fn serializes_existing_ref_parent_for_incremental_import() {
    let commit = FastImportCommit {
        mark: 1,
        refname: "refs/remotes/origin/trunk".to_string(),
        author: "A <a@example.com>".to_string(),
        committer: "A <a@example.com>".to_string(),
        timestamp: 1,
        timezone_offset: "+0000".to_string(),
        message: "incremental".to_string(),
        parent_mark: None,
        parent_ref: Some("refs/remotes/origin/trunk".to_string()),
        changes: vec![FileChange::Modify {
            path: "file.txt".to_string(),
            mode: "100644".to_string(),
            content: b"content\n".to_vec(),
        }],
    };

    let stream = String::from_utf8(FastImportStream::new().commit(&commit).finish()).unwrap();

    assert!(stream.contains("from refs/remotes/origin/trunk\n"));
}
