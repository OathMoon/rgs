#![cfg(feature = "svn-libsvn")]

use git_svn_rs_core::svn::SvnBackend;
use git_svn_rs_core::svn::libsvn::{
    LIBSVN_LINKED_PROBE_MESSAGE, LIBSVN_NOT_IMPLEMENTED_MESSAGE, LIBSVN_NOT_LINKED_MESSAGE,
    LibSvnBackend, LibSvnLinkStatus,
};

#[path = "support/svn_fixture.rs"]
mod svn_fixture;

use svn_fixture::{StandardSvnFixture, SvnToolPolicy, require_svn_tools};

#[test]
fn reports_feature_enabled_link_probe_state() {
    let availability = LibSvnBackend::availability();

    assert!(availability.feature_enabled);
    assert_eq!(availability.version, None);

    if cfg!(git_svn_rs_libsvn_linked) {
        assert_eq!(availability.link_status, LibSvnLinkStatus::Linked);
        assert_eq!(availability.detail, LIBSVN_LINKED_PROBE_MESSAGE);
    } else {
        assert_eq!(availability.link_status, LibSvnLinkStatus::NotLinked);
        assert_eq!(availability.detail, LIBSVN_NOT_LINKED_MESSAGE);
    }
}

#[test]
fn backend_reports_native_version_when_linked() {
    let backend = LibSvnBackend::new();

    if cfg!(git_svn_rs_libsvn_linked) {
        let version = backend.version().unwrap();
        assert!(!version.trim().is_empty());
        assert!(
            version.chars().any(|character| character.is_ascii_digit()),
            "{version}"
        );
    } else {
        assert_eq!(backend.version().unwrap_err(), LIBSVN_NOT_LINKED_MESSAGE);
    }
}

#[test]
fn backend_methods_report_unimplemented_or_not_linked() {
    let backend = LibSvnBackend::new();
    if cfg!(git_svn_rs_libsvn_linked) {
        let expected = "libsvn backend requires an SVN repository URL";

        assert_eq!(backend.uuid().unwrap_err(), expected);
        assert_eq!(backend.latest_revnum().unwrap_err(), expected);
        assert_eq!(
            backend.log(0, 1).unwrap_err(),
            LIBSVN_NOT_IMPLEMENTED_MESSAGE
        );
    } else {
        assert_eq!(backend.uuid().unwrap_err(), LIBSVN_NOT_LINKED_MESSAGE);
        assert_eq!(
            backend.latest_revnum().unwrap_err(),
            LIBSVN_NOT_LINKED_MESSAGE
        );
        assert_eq!(backend.log(0, 1).unwrap_err(), LIBSVN_NOT_LINKED_MESSAGE);
    }
}

#[test]
fn linked_backend_reads_file_repository_metadata() {
    if !cfg!(git_svn_rs_libsvn_linked) {
        return;
    }
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(reason)) => {
            eprintln!("{reason}");
            return;
        }
        Err(SvnToolPolicy::Fail(reason)) => panic!("{reason}"),
    }

    let fixture = StandardSvnFixture::create().unwrap();
    let backend = LibSvnBackend::for_url(fixture.url());

    assert_eq!(backend.latest_revnum().unwrap(), fixture.latest_revision());
    assert_eq!(backend.uuid().unwrap(), fixture.uuid());
}
