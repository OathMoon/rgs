use git_svn_rs_core::git_svn_id::GitSvnId;

#[test]
fn parses_git_svn_id_footer() {
    let parsed = GitSvnId::parse(
        "git-svn-id: https://svn.example/project/trunk@42 12345678-1234-1234-1234-123456789abc",
    )
    .unwrap();

    assert_eq!(parsed.url, "https://svn.example/project/trunk");
    assert_eq!(parsed.revision, 42);
    assert_eq!(parsed.uuid, "12345678-1234-1234-1234-123456789abc");
}

#[test]
fn formats_git_svn_id_footer() {
    let id = GitSvnId {
        url: "file:///repo/trunk".to_string(),
        revision: 7,
        uuid: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_string(),
    };

    assert_eq!(
        id.to_footer(),
        "git-svn-id: file:///repo/trunk@7 aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
    );
}

#[test]
fn rejects_missing_revision_separator() {
    let err = GitSvnId::parse("git-svn-id: file:///repo/trunk 7 uuid").unwrap_err();
    assert!(err.contains("missing @revision"));
}
