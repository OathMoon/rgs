mod support;

use support::svn_fixture::{
    StandardSvnFixture, SvnToolPolicy, missing_tools_policy, require_svn_tools,
};

#[test]
fn missing_svn_tools_are_skipped_unless_strict_compat_is_set() {
    assert_eq!(
        missing_tools_policy(false),
        SvnToolPolicy::Skip("skipping: svnadmin and svn are required".to_string())
    );
    assert_eq!(
        missing_tools_policy(true),
        SvnToolPolicy::Fail("svnadmin and svn are required".to_string())
    );
}

#[test]
fn standard_fixture_creates_trunk_branch_and_tag_revisions() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let fixture = StandardSvnFixture::create().unwrap();

    assert!(fixture.url().starts_with("file:///"));
    assert!(fixture.latest_revision() >= 4);
}
