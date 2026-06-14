use git_svn_rs_core::svn::mock::MockSvnBackend;
use git_svn_rs_core::svn::{RevisionEvent, SvnBackend};

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
