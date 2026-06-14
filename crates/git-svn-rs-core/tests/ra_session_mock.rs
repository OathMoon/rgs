use git_svn_rs_core::svn::editor::FetchEditor;
use git_svn_rs_core::svn::mock::MockRaSession;
use git_svn_rs_core::svn::ra::{RaSession, SvnNodeKind};

#[test]
fn mock_ra_session_exposes_check_path_get_dir_and_log() {
    let session = MockRaSession::standard_fixture("uuid");
    assert_eq!(session.uuid().unwrap(), "uuid");
    assert_eq!(
        session.check_path("trunk/src/lib.rs", 2).unwrap(),
        Some(SvnNodeKind::File)
    );
    assert!(
        session
            .get_dir("trunk", 2)
            .unwrap()
            .entries
            .contains_key("src")
    );
    assert_eq!(session.get_log(&["trunk"], 1, 2).unwrap().len(), 2);
}

#[test]
fn mock_ra_session_update_drives_fetch_editor_callbacks() {
    let session = MockRaSession::standard_fixture("uuid");
    let mut editor = RecordingFetchEditor::default();

    session.do_update("trunk", 2, &mut editor).unwrap();

    assert_eq!(
        editor.events,
        vec![
            "open_root:2",
            "add_directory:trunk",
            "add_directory:trunk/src",
            "add_file:trunk/src/lib.rs",
            "change_file_prop:trunk/src/lib.rs:svn:eol-style=LF",
            "apply_textdelta:trunk/src/lib.rs:29",
            "delete_entry:trunk/obsolete.txt@1",
            "close_edit",
        ]
    );
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

    fn add_directory(&mut self, path: &str, _copy_from: Option<(&str, u32)>) -> Result<(), String> {
        self.events.push(format!("add_directory:{path}"));
        Ok(())
    }

    fn add_file(&mut self, path: &str, _copy_from: Option<(&str, u32)>) -> Result<(), String> {
        self.events.push(format!("add_file:{path}"));
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
