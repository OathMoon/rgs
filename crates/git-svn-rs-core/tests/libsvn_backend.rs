#![cfg(feature = "svn-libsvn")]

use git_svn_rs_core::svn::SvnBackend;
use git_svn_rs_core::svn::libsvn::{LIBSVN_NOT_LINKED_MESSAGE, LibSvnBackend, LibSvnLinkStatus};

#[test]
fn reports_feature_enabled_but_not_linked() {
    let availability = LibSvnBackend::availability();

    assert!(availability.feature_enabled);
    assert_eq!(availability.link_status, LibSvnLinkStatus::NotLinked);
    assert_eq!(availability.version, None);
    assert_eq!(availability.detail, LIBSVN_NOT_LINKED_MESSAGE);
}

#[test]
fn backend_methods_return_not_linked_error() {
    let backend = LibSvnBackend::new();

    assert_eq!(backend.uuid().unwrap_err(), LIBSVN_NOT_LINKED_MESSAGE);
    assert_eq!(
        backend.latest_revnum().unwrap_err(),
        LIBSVN_NOT_LINKED_MESSAGE
    );
    assert_eq!(backend.log(0, 1).unwrap_err(), LIBSVN_NOT_LINKED_MESSAGE);
}
