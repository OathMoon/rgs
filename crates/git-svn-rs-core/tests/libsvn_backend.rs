#![cfg(feature = "svn-libsvn")]

use git_svn_rs_core::config::SvnRemoteConfig;
use git_svn_rs_core::fast_import::FileChange;
use git_svn_rs_core::fetch_editor::{FetchCommitPlan, SvnFetchEditor, TreeEntry};
use git_svn_rs_core::mapping::build_single_path;
use git_svn_rs_core::svn::SvnBackend;
use git_svn_rs_core::svn::editor::FetchEditor;
use git_svn_rs_core::svn::libsvn::{
    LIBSVN_LINKED_PROBE_MESSAGE, LIBSVN_NOT_LINKED_MESSAGE, LibSvnBackend, LibSvnLinkStatus,
};
use git_svn_rs_core::svn::ra::{RaSession, SvnNodeKind};
use git_svn_rs_core::svn::{ChangeAction, NodeKind};

#[path = "support/svn_fixture.rs"]
mod svn_fixture;

use svn_fixture::{
    StandardSvnFixture, SvnServe, SvnToolPolicy, require_svn_tools, require_svnserve,
};

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

        assert_eq!(SvnBackend::uuid(&backend).unwrap_err(), expected);
        assert_eq!(SvnBackend::latest_revnum(&backend).unwrap_err(), expected);
        assert_eq!(backend.log(0, 1).unwrap_err(), expected);
    } else {
        assert_eq!(
            SvnBackend::uuid(&backend).unwrap_err(),
            LIBSVN_NOT_LINKED_MESSAGE
        );
        assert_eq!(
            SvnBackend::latest_revnum(&backend).unwrap_err(),
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

    assert_eq!(
        SvnBackend::latest_revnum(&backend).unwrap(),
        fixture.latest_revision()
    );
    assert_eq!(SvnBackend::uuid(&backend).unwrap(), fixture.uuid());
}

#[test]
fn linked_backend_reads_metadata_with_config_dir_from_remote_config() {
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
    let config_dir = tempfile::tempdir().unwrap();
    let config = SvnRemoteConfig::new("svn", fixture.url(), build_single_path(""))
        .with_config_dir(config_dir.path().to_string_lossy());
    let backend = LibSvnBackend::from_config(&config);

    assert_eq!(
        SvnBackend::latest_revnum(&backend).unwrap(),
        fixture.latest_revision()
    );
    assert_eq!(SvnBackend::uuid(&backend).unwrap(), fixture.uuid());
}

#[test]
fn linked_backend_implements_ra_session_read_methods() {
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
    let session = LibSvnBackend::for_url(fixture.url());

    assert_eq!(RaSession::url(&session), fixture.url());
    assert_eq!(RaSession::repos_root(&session), fixture.url());
    assert_eq!(
        session.check_path("trunk/src/lib.rs", 2).unwrap(),
        Some(SvnNodeKind::File)
    );
    assert_eq!(
        session.check_path("trunk", 2).unwrap(),
        Some(SvnNodeKind::Directory)
    );
    assert_eq!(session.check_path("trunk/missing", 2).unwrap(), None);

    let trunk = session.get_dir("trunk", 2).unwrap();
    assert_eq!(trunk.entries["src"].kind, SvnNodeKind::Directory);
    assert_eq!(trunk.entries["run.sh"].kind, SvnNodeKind::File);
    assert!(trunk.properties.is_empty());

    let log = session.get_log(&["trunk"], 1, 2).unwrap();
    assert!(log.iter().any(|revision| revision.revision == 2));
    assert!(!log.iter().any(|revision| revision.revision == 3));
}

#[test]
fn linked_backend_get_dir_reads_directory_properties() {
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
    let revision = fixture
        .set_trunk_dir_property("custom:dir-prop", "dir-value")
        .unwrap();
    let session = LibSvnBackend::for_url(fixture.url());

    let trunk = session.get_dir("trunk", revision).unwrap();

    assert_eq!(
        trunk.properties.get("custom:dir-prop").map(String::as_str),
        Some("dir-value")
    );
}

#[test]
fn linked_backend_do_update_drives_fetch_editor_callbacks() {
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
    let session = LibSvnBackend::for_url(fixture.url());
    let mut editor = RecordingFetchEditor::default();

    session.do_update("trunk", 2, &mut editor).unwrap();

    assert!(editor.events.contains(&"open_root:2".to_string()));
    assert!(
        editor
            .events
            .contains(&"add_directory:trunk/src".to_string())
    );
    assert!(
        editor
            .events
            .contains(&"add_file:trunk/src/lib.rs".to_string())
    );
    assert!(editor.events.contains(&"add_file:trunk/run.sh".to_string()));
    assert!(
        editor
            .events
            .contains(&"change_file_prop:trunk/run.sh:svn:executable=*".to_string())
    );
    assert!(
        editor
            .events
            .contains(&"apply_textdelta:trunk/run.sh:18".to_string())
    );
    assert!(editor.events.contains(&"close_edit".to_string()));
}

#[test]
fn linked_backend_do_switch_drives_fetch_editor_callbacks() {
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
    let session = LibSvnBackend::for_url(fixture.url());
    let mut editor = RecordingFetchEditor::default();

    session
        .do_switch(
            "branches/main",
            3,
            &format!("{}/trunk", fixture.url()),
            &mut editor,
        )
        .unwrap();

    assert!(editor.events.contains(&"open_root:3".to_string()));
    assert!(
        editor
            .events
            .contains(&"add_directory:branches/main<-trunk@1".to_string())
    );
    assert!(
        editor
            .events
            .contains(&"add_file:branches/main/src/lib.rs<-trunk/src/lib.rs@2".to_string())
    );
    assert!(
        editor
            .events
            .contains(&"apply_textdelta:branches/main/src/lib.rs:29".to_string())
    );
    assert!(editor.events.contains(&"close_edit".to_string()));
}

#[test]
fn linked_backend_do_update_clears_removed_file_properties() {
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
    let revision = fixture.remove_executable_from_run_script().unwrap();
    let session = LibSvnBackend::for_url(fixture.url());
    let plan = FetchCommitPlan {
        mark: revision,
        refname: "refs/remotes/origin/trunk".to_string(),
        author: "alice <alice@example.com>".to_string(),
        committer: "alice <alice@example.com>".to_string(),
        timestamp: 1,
        message: "remove executable property".to_string(),
        parent_mark: None,
        parent_ref: None,
    };
    let mut editor = SvnFetchEditor::with_base_tree(
        plan,
        vec![TreeEntry::file("run.sh", "100755", "#!/bin/sh\necho hi\n")],
    )
    .with_path_prefix("trunk");

    session.do_update("trunk", revision, &mut editor).unwrap();
    let commit = editor.into_commit().unwrap();

    assert!(commit.changes.iter().any(|change| {
        matches!(
            change,
            FileChange::Modify { path, mode, content }
                if path == "run.sh"
                    && mode == "100644"
                    && content == b"#!/bin/sh\necho hi\n"
        )
    }));
}

#[test]
fn linked_backend_replays_local_svnserve_repository() {
    if !cfg!(git_svn_rs_libsvn_linked) {
        return;
    }
    match require_svn_tools().and_then(|()| require_svnserve()) {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(reason)) => {
            eprintln!("{reason}");
            return;
        }
        Err(SvnToolPolicy::Fail(reason)) => panic!("{reason}"),
    }

    let fixture = StandardSvnFixture::create().unwrap();
    let svnserve = SvnServe::start(fixture.root()).unwrap();
    let session = LibSvnBackend::for_url(svnserve.repo_url());
    let mut editor = RecordingFetchEditor::default();

    assert_eq!(
        SvnBackend::latest_revnum(&session).unwrap(),
        fixture.latest_revision()
    );
    assert_eq!(SvnBackend::uuid(&session).unwrap(), fixture.uuid());

    let trunk_log = session.get_log(&["trunk"], 1, 2).unwrap();
    assert!(trunk_log.iter().any(|revision| revision.revision == 2));
    assert!(!trunk_log.iter().any(|revision| revision.revision == 3));

    session.do_update("trunk", 2, &mut editor).unwrap();
    assert!(
        editor
            .events
            .contains(&"add_file:trunk/src/lib.rs".to_string())
    );
    assert!(
        editor
            .events
            .contains(&"apply_textdelta:trunk/src/lib.rs:29".to_string())
    );
    assert!(editor.events.contains(&"close_edit".to_string()));
}

#[test]
fn linked_backend_reads_authenticated_svnserve_with_credentials() {
    if !cfg!(git_svn_rs_libsvn_linked) {
        return;
    }
    match require_svn_tools().and_then(|()| require_svnserve()) {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(reason)) => {
            eprintln!("{reason}");
            return;
        }
        Err(SvnToolPolicy::Fail(reason)) => panic!("{reason}"),
    }

    let fixture = StandardSvnFixture::create().unwrap();
    fixture.require_basic_auth("alice", "secret").unwrap();
    let svnserve = SvnServe::start(fixture.root()).unwrap();
    let backend = LibSvnBackend::for_url(svnserve.repo_url()).with_credentials("alice", "secret");

    assert_eq!(
        SvnBackend::latest_revnum(&backend).unwrap(),
        fixture.latest_revision()
    );
    assert_eq!(SvnBackend::uuid(&backend).unwrap(), fixture.uuid());
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

    let branch = revisions
        .iter()
        .find(|revision| revision.revision == 3)
        .expect("branch copy revision should be present");
    assert_eq!(branch.message, "branch main");
    assert!(branch.changed_paths.iter().any(|path| {
        path.path == "/branches/main"
            && path.action == ChangeAction::Add
            && path.kind == NodeKind::Directory
            && path.copy_from_path.as_deref() == Some("/trunk")
            && path.copy_from_rev == Some(1)
    }));
    assert!(branch.changed_paths.iter().any(|path| {
        path.path == "/branches/main/src/lib.rs"
            && path.action == ChangeAction::Add
            && path.kind == NodeKind::File
            && path.copy_from_path.as_deref() == Some("/trunk/src/lib.rs")
            && path.copy_from_rev == Some(2)
            && path.content.as_deref() == Some(b"pub fn answer() -> u8 { 42 }\n".as_slice())
            && path.properties.is_empty()
    }));
    assert!(branch.changed_paths.iter().any(|path| {
        path.path == "/branches/main/run.sh"
            && path.action == ChangeAction::Add
            && path.kind == NodeKind::File
            && path.copy_from_path.as_deref() == Some("/trunk/run.sh")
            && path.copy_from_rev == Some(2)
            && path.content.as_deref() == Some(b"#!/bin/sh\necho hi\n".as_slice())
            && path.properties.get("svn:executable").map(String::as_str) == Some("*")
    }));
}

#[derive(Default)]
struct RecordingFetchEditor {
    events: Vec<String>,
}

impl FetchEditor for RecordingFetchEditor {
    fn open_root(&mut self, revision: u32) -> Result<(), String> {
        self.events.push(format!("open_root:{revision}"));
        Ok(())
    }

    fn add_directory(&mut self, path: &str, copy_from: Option<(&str, u32)>) -> Result<(), String> {
        self.events
            .push(format!("add_directory:{path}{}", copy_suffix(copy_from)));
        Ok(())
    }

    fn add_file(&mut self, path: &str, copy_from: Option<(&str, u32)>) -> Result<(), String> {
        self.events
            .push(format!("add_file:{path}{}", copy_suffix(copy_from)));
        Ok(())
    }

    fn delete_entry(&mut self, path: &str, revision: u32) -> Result<(), String> {
        self.events.push(format!("delete_entry:{path}@{revision}"));
        Ok(())
    }

    fn change_file_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        self.events.push(format!(
            "change_file_prop:{path}:{name}={}",
            value.unwrap_or_default()
        ));
        Ok(())
    }

    fn apply_textdelta(&mut self, path: &str, content: &[u8]) -> Result<(), String> {
        self.events
            .push(format!("apply_textdelta:{path}:{}", content.len()));
        Ok(())
    }

    fn close_edit(&mut self) -> Result<(), String> {
        self.events.push("close_edit".to_string());
        Ok(())
    }
}

fn copy_suffix(copy_from: Option<(&str, u32)>) -> String {
    copy_from
        .map(|(path, revision)| format!("<-{path}@{revision}"))
        .unwrap_or_default()
}
