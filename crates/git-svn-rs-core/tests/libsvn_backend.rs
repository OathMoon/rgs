#![cfg(feature = "svn-libsvn")]

use git_svn_rs_core::svn::SvnBackend;
use git_svn_rs_core::svn::libsvn::{
    LIBSVN_LINKED_PROBE_MESSAGE, LIBSVN_NOT_LINKED_MESSAGE, LibSvnBackend, LibSvnLinkStatus,
};
use git_svn_rs_core::svn::{ChangeAction, NodeKind};

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
        assert_eq!(backend.log(0, 1).unwrap_err(), expected);
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

#[test]
fn linked_backend_reads_log_metadata_and_changed_paths() {
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

    let revisions = backend.log(1, fixture.latest_revision()).unwrap();

    let layout = revisions
        .iter()
        .find(|revision| revision.revision == 1)
        .expect("layout revision should be present");
    assert_eq!(layout.message, "layout");
    assert!(!layout.author.is_empty());
    assert!(!layout.timestamp.is_empty());
    assert!(layout.changed_paths.iter().any(|path| {
        path.path == "/trunk"
            && path.action == ChangeAction::Add
            && path.kind == NodeKind::Directory
    }));

    let trunk = revisions
        .iter()
        .find(|revision| revision.revision == 2)
        .expect("trunk file revision should be present");
    assert_eq!(trunk.message, "add trunk file");
    assert!(trunk.changed_paths.iter().any(|path| {
        path.path == "/trunk/src/lib.rs"
            && path.action == ChangeAction::Add
            && path.kind == NodeKind::File
            && path.copy_from_path.is_none()
            && path.copy_from_rev.is_none()
            && path.content.as_deref() == Some(b"pub fn answer() -> u8 { 42 }\n".as_slice())
            && path.properties.is_empty()
    }));
    assert!(trunk.changed_paths.iter().any(|path| {
        path.path == "/trunk/run.sh"
            && path.action == ChangeAction::Add
            && path.kind == NodeKind::File
            && path.content.as_deref() == Some(b"#!/bin/sh\necho hi\n".as_slice())
            && path.properties.get("svn:executable").map(String::as_str) == Some("*")
    }));
    assert!(trunk.changed_paths.iter().any(|path| {
        path.path == "/trunk/link-to-lib"
            && path.action == ChangeAction::Add
            && path.kind == NodeKind::File
            && path.content.as_deref() == Some(b"link src/lib.rs".as_slice())
            && path.properties.get("svn:special").map(String::as_str) == Some("*")
    }));
}
