#[cfg(git_svn_rs_libsvn_linked)]
use super::super::auth::AuthRequest;
#[cfg(git_svn_rs_libsvn_linked)]
use super::auth::{SimplePromptBaton, prompt_simple_credentials};
#[cfg(git_svn_rs_libsvn_linked)]
use super::ra::{get_file, receive_log_entry, stringbuf_bytes};
use super::*;
#[cfg(git_svn_rs_libsvn_linked)]
use crate::svn::{ChangeAction, NodeKind};

#[cfg(git_svn_rs_libsvn_linked)]
use std::fs;
#[cfg(git_svn_rs_libsvn_linked)]
use std::os::raw::{c_char, c_void};
#[cfg(git_svn_rs_libsvn_linked)]
use std::path::Path;
#[cfg(git_svn_rs_libsvn_linked)]
use std::process::Command;
#[cfg(git_svn_rs_libsvn_linked)]
use std::slice;

#[test]
fn backend_without_url_reports_expected_error() {
    let backend = LibSvnBackend::new();

    #[cfg(git_svn_rs_libsvn_linked)]
    assert_eq!(
        SvnBackend::uuid(&backend).unwrap_err(),
        "libsvn backend requires an SVN repository URL"
    );

    #[cfg(not(git_svn_rs_libsvn_linked))]
    assert_eq!(
        SvnBackend::uuid(&backend).unwrap_err(),
        LibSvnBackend::unavailable_message()
    );
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn auth_prompt_panic_does_not_cross_ffi_boundary() {
    struct PanicPrompt;

    impl AuthPrompt for PanicPrompt {
        fn simple(&self, _request: AuthRequest) -> Result<super::super::auth::Credentials, String> {
            panic!("prompt panic")
        }
    }

    AprRuntime::initialize().unwrap();
    let pool = AprRuntime.create_pool().unwrap();
    let prompt: Arc<dyn AuthPrompt> = Arc::new(PanicPrompt);
    let mut baton = SimplePromptBaton {
        prompt: &prompt,
        no_auth_cache: true,
    };
    let mut credentials = ptr::dangling_mut::<svn_auth_cred_simple_t>();

    let error = unsafe {
        prompt_simple_credentials(
            &mut credentials,
            (&mut baton as *mut SimplePromptBaton).cast::<c_void>(),
            ptr::null(),
            ptr::null(),
            0,
            pool.as_ptr(),
        )
    };

    assert!(error.is_null());
    assert!(credentials.is_null());
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn production_callbacks_return_owned_errors_for_invalid_inputs() {
    AprRuntime::initialize().unwrap();
    let _pool = AprRuntime.create_pool().unwrap();
    let error = unsafe { receive_log_entry(ptr::null_mut(), ptr::null_mut(), ptr::null_mut()) };
    assert!(!error.is_null());
    let detail = unsafe { svn_error_detail(error, "receive_log_entry") };
    unsafe { svn_error_clear(error) };
    assert!(detail.contains("null baton or entry"), "{detail}");

    let error = unsafe {
        prompt_simple_credentials(
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
            ptr::null(),
            0,
            ptr::null_mut(),
        )
    };
    assert!(!error.is_null());
    let detail = unsafe { svn_error_detail(error, "prompt_simple_credentials") };
    unsafe { svn_error_clear(error) };
    assert!(detail.contains("null input or pool"), "{detail}");
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn callback_error_falls_back_for_interior_nul_without_panicking() {
    AprRuntime::initialize().unwrap();
    let _pool = AprRuntime.create_pool().unwrap();
    let error = unsafe { callback_error_message("bad\0callback") };
    assert!(!error.is_null());
    let detail = unsafe { svn_error_detail(error, "callback") };
    unsafe { svn_error_clear(error) };
    assert!(detail.contains("libsvn callback failed"), "{detail}");
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn svn_call_reports_child_error_messages() {
    let child_message = CString::new("child detail").unwrap();
    let parent_message = CString::new("parent detail").unwrap();
    let mut child = svn_error_t {
        apr_err: 2,
        message: child_message.as_ptr(),
        child: ptr::null_mut(),
        pool: ptr::null_mut(),
        file: ptr::null(),
        line: 0,
    };
    let mut parent = svn_error_t {
        apr_err: 1,
        message: parent_message.as_ptr(),
        child: &mut child,
        pool: ptr::null_mut(),
        file: ptr::null(),
        line: 0,
    };

    let error = unsafe { svn_error_detail(&mut parent, "svn_test_call") };

    assert!(error.contains("parent detail"), "{error}");
    assert!(error.contains("child detail"), "{error}");
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn svn_call_error_detail_falls_back_to_context() {
    let detail = unsafe { svn_error_detail(ptr::null_mut(), "svn_test_call") };

    assert_eq!(detail, "svn_test_call");
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn default_delta_editor_exposes_textdelta_stream_slot() {
    AprRuntime::initialize().unwrap();
    let apr = AprRuntime;
    let pool = apr.create_pool().unwrap();

    let editor = unsafe { svn_delta_default_editor(pool.as_ptr()) };

    assert!(!editor.is_null());
    assert!(unsafe { (*editor).apply_textdelta_stream }.is_some());
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_reporter_finishes_with_default_delta_editor() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);

    drive_update_report(&backend, 2, ptr::null_mut(), |_| {}).unwrap();
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_reporter_can_abort_default_delta_editor_report() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);

    backend
        .with_session(|session, pool| unsafe {
            let editor = svn_delta_default_editor(pool);
            assert!(!editor.is_null());

            let mut reporter: *const SvnRaReporter3T = ptr::null();
            let mut report_baton: *mut c_void = ptr::null_mut();
            let target = CString::new("trunk").unwrap();
            svn_call(
                svn_ra_do_update3(
                    session,
                    &mut reporter,
                    &mut report_baton,
                    2,
                    target.as_ptr(),
                    SVN_DEPTH_INFINITY,
                    1,
                    0,
                    editor,
                    ptr::null_mut(),
                    pool,
                    pool,
                ),
                "svn_ra_do_update3",
            )?;

            assert!(!reporter.is_null());
            let abort_report = (*reporter)
                .abort_report
                .ok_or_else(|| "libsvn returned a reporter without abort_report".to_string())?;

            svn_call(
                abort_report(report_baton, pool),
                "svn_ra_reporter3_t.abort_report",
            )
        })
        .unwrap();
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_reporter_exposes_delete_and_link_path_callbacks() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);

    backend
        .with_session(|session, pool| unsafe {
            let editor = svn_delta_default_editor(pool);
            assert!(!editor.is_null());

            let mut reporter: *const SvnRaReporter3T = ptr::null();
            let mut report_baton: *mut c_void = ptr::null_mut();
            let target = CString::new("trunk").unwrap();
            svn_call(
                svn_ra_do_update3(
                    session,
                    &mut reporter,
                    &mut report_baton,
                    2,
                    target.as_ptr(),
                    SVN_DEPTH_INFINITY,
                    1,
                    0,
                    editor,
                    ptr::null_mut(),
                    pool,
                    pool,
                ),
                "svn_ra_do_update3",
            )?;

            assert!(!reporter.is_null());
            assert!((*reporter).delete_path.is_some());
            assert!((*reporter).link_path.is_some());

            let abort_report = (*reporter)
                .abort_report
                .ok_or_else(|| "libsvn returned a reporter without abort_report".to_string())?;
            svn_call(
                abort_report(report_baton, pool),
                "svn_ra_reporter3_t.abort_report",
            )
        })
        .unwrap();
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_invokes_patched_delta_editor_callbacks() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);
    let mut baton = RecordingEditorBaton::default();

    drive_update_report(
        &backend,
        2,
        &mut baton as *mut RecordingEditorBaton as *mut c_void,
        |editor| unsafe {
            (*editor).set_target_revision = Some(record_target_revision);
            (*editor).close_edit = Some(record_close_edit);
        },
    )
    .unwrap();

    assert_eq!(baton.target_revision, 2);
    assert!(baton.closed);
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_invokes_patched_directory_delta_callbacks() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);
    let mut baton = RecordingEditorBaton::default();

    drive_update_report(
        &backend,
        2,
        &mut baton as *mut RecordingEditorBaton as *mut c_void,
        |editor| unsafe {
            (*editor).open_root = Some(record_open_root);
            (*editor).close_directory = Some(record_close_directory);
        },
    )
    .unwrap();

    assert!(baton.root_opened);
    assert_eq!(baton.root_base_revision, 0);
    assert!(baton.closed_directories > 0);
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_invokes_patched_file_delta_callbacks() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);
    let mut baton = RecordingEditorBaton::default();

    drive_update_report(
        &backend,
        2,
        &mut baton as *mut RecordingEditorBaton as *mut c_void,
        |editor| unsafe {
            (*editor).open_root = Some(record_open_root);
            (*editor).add_file = Some(record_add_file);
            (*editor).close_file = Some(record_close_file);
        },
    )
    .unwrap();

    assert_eq!(baton.added_files, ["trunk/file.txt"]);
    assert_eq!(baton.closed_files, 1);
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_invokes_patched_textdelta_callbacks() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);
    let mut baton = RecordingEditorBaton::default();

    drive_update_report(
        &backend,
        2,
        &mut baton as *mut RecordingEditorBaton as *mut c_void,
        |editor| unsafe {
            (*editor).open_root = Some(record_open_root);
            (*editor).add_file = Some(record_add_file);
            (*editor).apply_textdelta = Some(record_apply_textdelta);
            (*editor).close_file = Some(record_close_file);
        },
    )
    .unwrap();

    assert_eq!(baton.apply_textdelta_callbacks, 1);
    assert!(baton.textdelta_windows > 0);
    assert_eq!(baton.textdelta_done_windows, 1);
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_textdelta_windows_can_be_applied_to_fulltext_buffer() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    AprRuntime::initialize().unwrap();
    let apr = AprRuntime;
    let pool = apr.create_pool().unwrap();
    let backend = LibSvnBackend::for_url(repo_url);
    let mut baton = RecordingEditorBaton {
        textdelta_buffer: unsafe { svn_stringbuf_create_empty(pool.as_ptr()) },
        ..RecordingEditorBaton::default()
    };

    assert!(!baton.textdelta_buffer.is_null());
    drive_update_report(
        &backend,
        2,
        &mut baton as *mut RecordingEditorBaton as *mut c_void,
        |editor| unsafe {
            (*editor).open_root = Some(record_open_root);
            (*editor).add_directory = Some(record_add_directory);
            (*editor).open_directory = Some(record_open_directory);
            (*editor).add_file = Some(record_add_file);
            (*editor).apply_textdelta = Some(record_apply_textdelta_to_buffer);
            (*editor).close_file = Some(record_close_file);
        },
    )
    .unwrap();

    assert_eq!(baton.textdelta_applied_to_buffer, 1);
    assert_eq!(
        unsafe { stringbuf_bytes(baton.textdelta_buffer) },
        b"hello\n"
    );
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_callbacks_drive_fetch_editor_for_initial_file_add() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);
    let mut editor = RecordingFetchEditor::default();

    drive_fetch_editor_update_report_for_test(&backend, "trunk", 2, 0, 1, &mut editor).unwrap();

    assert!(editor.events.contains(&"open_root:2".to_string()));
    assert!(
        editor
            .events
            .contains(&"add_file:trunk/file.txt".to_string())
    );
    assert!(
        editor
            .events
            .contains(&"apply_textdelta:trunk/file.txt:6".to_string())
    );
    assert!(editor.events.contains(&"close_edit".to_string()));
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_callbacks_drive_fetch_editor_for_incremental_file_edit() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);
    let mut editor = RecordingFetchEditor::default();

    drive_fetch_editor_update_report_for_test(&backend, "trunk", 3, 2, 0, &mut editor).unwrap();

    assert!(editor.events.contains(&"open_root:3".to_string()));
    assert!(
        editor
            .events
            .contains(&"apply_textdelta:trunk/file.txt:12".to_string()),
        "{:?}",
        editor.events
    );
    assert!(editor.events.contains(&"close_edit".to_string()));
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_callbacks_drive_fetch_editor_for_file_property_change() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);
    let mut editor = RecordingFetchEditor::default();

    drive_fetch_editor_update_report_for_test(&backend, "trunk", 4, 3, 0, &mut editor).unwrap();

    assert!(editor.events.contains(&"open_root:4".to_string()));
    assert!(
        editor
            .events
            .contains(&"change_file_prop:trunk/file.txt:svn:needs-lock=*".to_string()),
        "{:?}",
        editor.events
    );
    assert!(
        !editor
            .events
            .iter()
            .any(|event| event.starts_with("apply_textdelta:trunk/file.txt:")),
        "{:?}",
        editor.events
    );
    assert!(editor.events.contains(&"close_edit".to_string()));
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_callbacks_drive_fetch_editor_for_directory_property_change() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);
    let mut editor = RecordingFetchEditor::default();

    drive_fetch_editor_update_report_for_test(&backend, "trunk", 5, 4, 0, &mut editor).unwrap();

    assert!(editor.events.contains(&"open_root:5".to_string()));
    assert!(
        editor
            .events
            .contains(&"change_directory_prop:trunk:svn:ignore=target\n".to_string()),
        "{:?}",
        editor.events
    );
    assert!(
        !editor
            .events
            .iter()
            .any(|event| event.starts_with("change_directory_prop:trunk:svn:entry:")),
        "{:?}",
        editor.events
    );
    assert!(editor.events.contains(&"close_edit".to_string()));
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_callbacks_drive_fetch_editor_for_delete_entry() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);
    let mut editor = RecordingFetchEditor::default();

    drive_fetch_editor_update_report_for_test(&backend, "trunk", 6, 5, 0, &mut editor).unwrap();

    assert!(editor.events.contains(&"open_root:6".to_string()));
    assert!(
        editor
            .events
            .contains(&"delete_entry:trunk/file.txt@6".to_string()),
        "{:?}",
        editor.events
    );
    assert!(editor.events.contains(&"close_edit".to_string()));
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_callbacks_drive_fetch_editor_for_nested_directory_property_change() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);
    let mut editor = RecordingFetchEditor::default();

    drive_fetch_editor_update_report_for_test(&backend, "trunk", 9, 8, 0, &mut editor).unwrap();

    assert!(editor.events.contains(&"open_root:9".to_string()));
    assert!(
        editor
            .events
            .contains(&"change_directory_prop:trunk/subdir:svn:ignore=nested-target\n".to_string()),
        "{:?}",
        editor.events
    );
    assert!(editor.events.contains(&"close_edit".to_string()));
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn default_delta_editor_accepts_patched_textdelta_stream_callback() {
    AprRuntime::initialize().unwrap();
    let apr = AprRuntime;
    let pool = apr.create_pool().unwrap();
    let editor = unsafe { svn_delta_default_editor(pool.as_ptr()) };

    assert!(!editor.is_null());
    let error = unsafe {
        (*editor).apply_textdelta_stream = Some(record_apply_textdelta_stream);
        let apply_textdelta_stream = (*editor).apply_textdelta_stream.unwrap();
        apply_textdelta_stream(
            editor,
            ptr::null_mut(),
            ptr::null(),
            Some(record_open_txdelta_stream),
            ptr::null_mut(),
            pool.as_ptr(),
        )
    };

    assert!(error.is_null());
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn default_delta_editor_accepts_patched_absent_and_abort_callbacks() {
    AprRuntime::initialize().unwrap();
    let apr = AprRuntime;
    let pool = apr.create_pool().unwrap();
    let editor = unsafe { svn_delta_default_editor(pool.as_ptr()) };
    let mut baton = RecordingEditorBaton::default();
    let baton_ptr = (&mut baton as *mut RecordingEditorBaton).cast::<c_void>();

    assert!(!editor.is_null());
    let error = unsafe {
        (*editor).absent_directory = Some(record_absent_directory);
        (*editor).absent_file = Some(record_absent_file);
        (*editor).abort_edit = Some(record_abort_edit);

        let absent_path = CString::new("trunk/missing").unwrap();
        let absent_directory = (*editor).absent_directory.unwrap();
        let absent_error = absent_directory(absent_path.as_ptr(), baton_ptr, pool.as_ptr());
        assert!(absent_error.is_null());

        let absent_file = (*editor).absent_file.unwrap();
        let absent_error = absent_file(absent_path.as_ptr(), baton_ptr, pool.as_ptr());
        assert!(absent_error.is_null());

        let abort_edit = (*editor).abort_edit.unwrap();
        abort_edit(baton_ptr, pool.as_ptr())
    };

    assert!(error.is_null());
    assert_eq!(baton.absent_directories, 1);
    assert_eq!(baton.absent_files, 1);
    assert_eq!(baton.aborted_edits, 1);
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_invokes_patched_open_file_callback_for_incremental_edit() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);
    let mut baton = RecordingEditorBaton::default();

    drive_update_report_from_base(
        &backend,
        3,
        2,
        0,
        &mut baton as *mut RecordingEditorBaton as *mut c_void,
        |editor| unsafe {
            (*editor).open_root = Some(record_open_root);
            (*editor).open_file = Some(record_open_file);
            (*editor).apply_textdelta = Some(record_apply_textdelta);
            (*editor).close_file = Some(record_close_file);
        },
    )
    .unwrap();

    assert_eq!(baton.opened_files, ["trunk/file.txt@2"]);
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_invokes_patched_file_property_callback() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);
    let mut baton = RecordingEditorBaton::default();

    drive_update_report_from_base(
        &backend,
        4,
        3,
        0,
        &mut baton as *mut RecordingEditorBaton as *mut c_void,
        |editor| unsafe {
            (*editor).open_root = Some(record_open_root);
            (*editor).open_file = Some(record_open_file);
            (*editor).change_file_prop = Some(record_change_file_prop);
            (*editor).close_file = Some(record_close_file);
        },
    )
    .unwrap();

    assert!(
        baton
            .file_properties
            .iter()
            .any(|property| property == "svn:needs-lock=*")
    );
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_invokes_patched_directory_property_callback() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);
    let mut baton = RecordingEditorBaton::default();

    drive_update_report_from_base(
        &backend,
        5,
        4,
        0,
        &mut baton as *mut RecordingEditorBaton as *mut c_void,
        |editor| unsafe {
            (*editor).open_root = Some(record_open_root);
            (*editor).change_dir_prop = Some(record_change_dir_prop);
            (*editor).close_directory = Some(record_close_directory);
        },
    )
    .unwrap();

    assert!(
        baton
            .directory_properties
            .iter()
            .any(|property| property == "svn:ignore=target\n")
    );
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_invokes_patched_delete_entry_callback() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);
    let mut baton = RecordingEditorBaton::default();

    drive_update_report_from_base(
        &backend,
        6,
        5,
        0,
        &mut baton as *mut RecordingEditorBaton as *mut c_void,
        |editor| unsafe {
            (*editor).open_root = Some(record_open_root);
            (*editor).delete_entry = Some(record_delete_entry);
            (*editor).close_directory = Some(record_close_directory);
        },
    )
    .unwrap();

    assert_eq!(baton.deleted_entries, ["trunk/file.txt@6"]);
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_invokes_patched_add_directory_callback() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);
    let mut baton = RecordingEditorBaton::default();

    drive_update_report_from_base(
        &backend,
        7,
        6,
        0,
        &mut baton as *mut RecordingEditorBaton as *mut c_void,
        |editor| unsafe {
            (*editor).open_root = Some(record_open_root);
            (*editor).add_directory = Some(record_add_directory);
            (*editor).close_directory = Some(record_close_directory);
        },
    )
    .unwrap();

    assert_eq!(baton.added_directories, ["trunk/subdir"]);
}

#[cfg(git_svn_rs_libsvn_linked)]
#[test]
fn native_update_invokes_patched_open_directory_callback() {
    let (_tmp, repo_url) = match create_minimal_svn_repository() {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let backend = LibSvnBackend::for_url(repo_url);
    let mut baton = RecordingEditorBaton::default();

    drive_update_report_from_base(
        &backend,
        8,
        7,
        0,
        &mut baton as *mut RecordingEditorBaton as *mut c_void,
        |editor| unsafe {
            (*editor).open_root = Some(record_open_root);
            (*editor).open_directory = Some(record_open_directory);
            (*editor).add_file = Some(record_add_file);
            (*editor).close_file = Some(record_close_file);
            (*editor).close_directory = Some(record_close_directory);
        },
    )
    .unwrap();

    assert!(
        baton
            .opened_directories
            .iter()
            .any(|directory| directory == "trunk/subdir@7")
    );
}

#[cfg(git_svn_rs_libsvn_linked)]
fn drive_update_report(
    backend: &LibSvnBackend,
    revision: c_long,
    update_baton: *mut c_void,
    configure_editor: impl FnOnce(*mut SvnDeltaEditorT),
) -> Result<(), String> {
    drive_update_report_from_base(backend, revision, 0, 1, update_baton, configure_editor)
}

#[cfg(git_svn_rs_libsvn_linked)]
fn drive_update_report_from_base(
    backend: &LibSvnBackend,
    revision: c_long,
    base_revision: c_long,
    start_empty: c_int,
    update_baton: *mut c_void,
    configure_editor: impl FnOnce(*mut SvnDeltaEditorT),
) -> Result<(), String> {
    backend.with_session(|session, pool| unsafe {
        let editor = svn_delta_default_editor(pool);
        if editor.is_null() {
            return Err("svn_delta_default_editor returned null".to_string());
        }
        if !update_baton.is_null() {
            (*editor).open_root = Some(record_open_root);
            (*editor).add_directory = Some(record_add_directory);
            (*editor).open_directory = Some(record_open_directory);
            (*editor).add_file = Some(record_add_file);
            (*editor).open_file = Some(record_open_file);
        }
        configure_editor(editor);

        let mut reporter: *const SvnRaReporter3T = ptr::null();
        let mut report_baton: *mut c_void = ptr::null_mut();
        let target = CString::new("trunk").map_err(|_| "static update target contains NUL")?;
        svn_call(
            svn_ra_do_update3(
                session,
                &mut reporter,
                &mut report_baton,
                revision,
                target.as_ptr(),
                SVN_DEPTH_INFINITY,
                1,
                0,
                editor,
                update_baton,
                pool,
                pool,
            ),
            "svn_ra_do_update3",
        )?;

        if reporter.is_null() {
            return Err("libsvn returned a null update reporter".to_string());
        }
        let set_path = (*reporter)
            .set_path
            .ok_or_else(|| "libsvn returned a reporter without set_path".to_string())?;
        let finish_report = (*reporter)
            .finish_report
            .ok_or_else(|| "libsvn returned a reporter without finish_report".to_string())?;

        let empty_path = CString::new("").map_err(|_| "static empty path contains NUL")?;
        svn_call(
            set_path(
                report_baton,
                empty_path.as_ptr(),
                base_revision,
                SVN_DEPTH_INFINITY,
                start_empty,
                ptr::null(),
                pool,
            ),
            "svn_ra_reporter3_t.set_path",
        )?;
        svn_call(
            finish_report(report_baton, pool),
            "svn_ra_reporter3_t.finish_report",
        )
    })
}

#[cfg(git_svn_rs_libsvn_linked)]
fn drive_fetch_editor_update_report_for_test(
    backend: &LibSvnBackend,
    target: &str,
    revision: c_long,
    base_revision: c_long,
    start_empty: c_int,
    fetch_editor: &mut dyn FetchEditor,
) -> Result<(), String> {
    let revision_number = u32::try_from(revision)
        .map_err(|_| format!("SVN revision {revision} does not fit in u32"))?;
    let base_file_paths = backend
        .get_log(&[target], revision_number, revision_number)?
        .into_iter()
        .flat_map(|revision| revision.changed_paths)
        .filter(|path| {
            matches!(path.action, ChangeAction::Modify)
                && matches!(path.kind, NodeKind::File | NodeKind::Symlink)
        })
        .map(|path| path.path)
        .collect::<Vec<_>>();

    backend.with_session(|session, pool| unsafe {
        let mut base_file_contents = BTreeMap::new();
        for path in &base_file_paths {
            let (content, _) = get_file(session, pool, path, base_revision)?;
            base_file_contents.insert(editor_path(path), content);
        }

        let editor = svn_delta_default_editor(pool);
        if editor.is_null() {
            return Err("svn_delta_default_editor returned null".to_string());
        }

        let mut baton = FetchEditorUpdateBaton {
            editor: fetch_editor,
            active_directory_paths: vec![target.to_string()],
            active_file_path: None,
            base_file_contents,
            active_textdelta_source_buffer: ptr::null_mut(),
            active_textdelta_buffer: ptr::null_mut(),
            error: None,
        };
        (*editor).set_target_revision = Some(fetch_set_target_revision);
        (*editor).open_root = Some(fetch_open_root);
        (*editor).delete_entry = Some(fetch_delete_entry);
        (*editor).add_directory = Some(fetch_add_directory);
        (*editor).open_directory = Some(fetch_open_directory);
        (*editor).close_directory = Some(fetch_close_directory);
        (*editor).add_file = Some(fetch_add_file);
        (*editor).open_file = Some(fetch_open_file);
        (*editor).apply_textdelta = Some(fetch_apply_textdelta);
        (*editor).change_dir_prop = Some(fetch_change_dir_prop);
        (*editor).change_file_prop = Some(fetch_change_file_prop);
        (*editor).close_file = Some(fetch_close_file);
        (*editor).close_edit = Some(fetch_close_edit);

        let mut reporter: *const SvnRaReporter3T = ptr::null();
        let mut report_baton: *mut c_void = ptr::null_mut();
        let target = CString::new(target).map_err(|_| "SVN update target contains NUL")?;
        svn_call(
            svn_ra_do_update3(
                session,
                &mut reporter,
                &mut report_baton,
                revision,
                target.as_ptr(),
                SVN_DEPTH_INFINITY,
                1,
                0,
                editor,
                (&mut baton as *mut FetchEditorUpdateBaton).cast::<c_void>(),
                pool,
                pool,
            ),
            "svn_ra_do_update3",
        )?;

        if reporter.is_null() {
            return Err("libsvn returned a null update reporter".to_string());
        }
        let set_path = (*reporter)
            .set_path
            .ok_or_else(|| "libsvn returned a reporter without set_path".to_string())?;
        let finish_report = (*reporter)
            .finish_report
            .ok_or_else(|| "libsvn returned a reporter without finish_report".to_string())?;

        let empty_path = CString::new("").map_err(|_| "static empty path contains NUL")?;
        svn_call(
            set_path(
                report_baton,
                empty_path.as_ptr(),
                base_revision,
                SVN_DEPTH_INFINITY,
                start_empty,
                ptr::null(),
                pool,
            ),
            "svn_ra_reporter3_t.set_path",
        )?;
        svn_call(
            finish_report(report_baton, pool),
            "svn_ra_reporter3_t.finish_report",
        )?;

        baton.error.map_or(Ok(()), Err)
    })
}

#[cfg(git_svn_rs_libsvn_linked)]
#[derive(Default)]
struct RecordingEditorBaton {
    target_revision: c_long,
    closed: bool,
    root_opened: bool,
    root_base_revision: c_long,
    closed_directories: usize,
    added_files: Vec<String>,
    closed_files: usize,
    opened_files: Vec<String>,
    file_properties: Vec<String>,
    directory_properties: Vec<String>,
    deleted_entries: Vec<String>,
    added_directories: Vec<String>,
    opened_directories: Vec<String>,
    apply_textdelta_callbacks: usize,
    textdelta_applied_to_buffer: usize,
    textdelta_windows: usize,
    textdelta_done_windows: usize,
    absent_directories: usize,
    absent_files: usize,
    aborted_edits: usize,
    textdelta_buffer: *mut svn_stringbuf_t,
}

#[cfg(git_svn_rs_libsvn_linked)]
struct FetchEditorUpdateBaton<'a> {
    editor: &'a mut dyn FetchEditor,
    active_directory_paths: Vec<String>,
    active_file_path: Option<String>,
    base_file_contents: BTreeMap<String, Vec<u8>>,
    active_textdelta_source_buffer: *mut svn_stringbuf_t,
    active_textdelta_buffer: *mut svn_stringbuf_t,
    error: Option<String>,
}

#[cfg(git_svn_rs_libsvn_linked)]
#[derive(Default)]
struct RecordingFetchEditor {
    events: Vec<String>,
}

#[cfg(git_svn_rs_libsvn_linked)]
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

    fn change_directory_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        self.events.push(format!(
            "change_directory_prop:{path}:{name}={}",
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

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_target_revision(
    edit_baton: *mut c_void,
    target_revision: c_long,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if edit_baton.is_null() {
        return ptr::null_mut();
    }
    let baton = unsafe { &mut *(edit_baton as *mut RecordingEditorBaton) };
    baton.target_revision = target_revision;
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn fetch_set_target_revision(
    edit_baton: *mut c_void,
    target_revision: c_long,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    let baton = unsafe { &mut *(edit_baton as *mut FetchEditorUpdateBaton) };
    let revision = match u32::try_from(target_revision) {
        Ok(revision) => revision,
        Err(_) => {
            baton.error = Some(format!(
                "SVN target revision {target_revision} does not fit in u32"
            ));
            return ptr::null_mut();
        }
    };
    if let Err(error) = baton.editor.open_root(revision) {
        baton.error = Some(error);
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn fetch_open_root(
    edit_baton: *mut c_void,
    _base_revision: c_long,
    _dir_pool: *mut AprPoolT,
    root_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    unsafe {
        *root_baton = edit_baton;
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn fetch_delete_entry(
    path: *const c_char,
    revision: c_long,
    parent_baton: *mut c_void,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if parent_baton.is_null() {
        return ptr::null_mut();
    }
    let baton = unsafe { &mut *(parent_baton as *mut FetchEditorUpdateBaton) };
    let revision = match u32::try_from(revision) {
        Ok(revision) => revision,
        Err(_) => {
            baton.error = Some(format!(
                "SVN delete revision {revision} does not fit in u32"
            ));
            return ptr::null_mut();
        }
    };
    let path = unsafe { CStr::from_ptr(path) }.to_string_lossy();
    if let Err(error) = baton.editor.delete_entry(&path, revision) {
        baton.error = Some(error);
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn fetch_add_directory(
    path: *const c_char,
    parent_baton: *mut c_void,
    _copyfrom_path: *const c_char,
    _copyfrom_revision: c_long,
    _dir_pool: *mut AprPoolT,
    child_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    if !parent_baton.is_null() {
        let baton = unsafe { &mut *(parent_baton as *mut FetchEditorUpdateBaton) };
        let path = unsafe { CStr::from_ptr(path) }
            .to_string_lossy()
            .into_owned();
        if let Err(error) = baton.editor.add_directory(&path, None) {
            baton.error = Some(error);
        }
        baton.active_directory_paths.push(path);
    }
    unsafe {
        *child_baton = parent_baton;
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn fetch_open_directory(
    path: *const c_char,
    parent_baton: *mut c_void,
    _base_revision: c_long,
    _dir_pool: *mut AprPoolT,
    child_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    if !parent_baton.is_null() {
        let baton = unsafe { &mut *(parent_baton as *mut FetchEditorUpdateBaton) };
        baton.active_directory_paths.push(
            unsafe { CStr::from_ptr(path) }
                .to_string_lossy()
                .into_owned(),
        );
    }
    unsafe {
        *child_baton = parent_baton;
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn fetch_close_directory(
    dir_baton: *mut c_void,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if dir_baton.is_null() {
        return ptr::null_mut();
    }
    let baton = unsafe { &mut *(dir_baton as *mut FetchEditorUpdateBaton) };
    baton.active_directory_paths.pop();
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn fetch_add_file(
    path: *const c_char,
    parent_baton: *mut c_void,
    _copyfrom_path: *const c_char,
    _copyfrom_revision: c_long,
    file_pool: *mut AprPoolT,
    file_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    if !parent_baton.is_null() {
        let baton = unsafe { &mut *(parent_baton as *mut FetchEditorUpdateBaton) };
        let path = unsafe { CStr::from_ptr(path) }
            .to_string_lossy()
            .into_owned();
        if let Err(error) = baton.editor.add_file(&path, None) {
            baton.error = Some(error);
        }
        baton.active_file_path = Some(path);
        baton.active_textdelta_source_buffer = ptr::null_mut();
        baton.active_textdelta_buffer = unsafe { svn_stringbuf_create_empty(file_pool) };
    }
    unsafe {
        *file_baton = parent_baton;
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn fetch_open_file(
    path: *const c_char,
    parent_baton: *mut c_void,
    _base_revision: c_long,
    _file_pool: *mut AprPoolT,
    file_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    if !parent_baton.is_null() {
        let baton = unsafe { &mut *(parent_baton as *mut FetchEditorUpdateBaton) };
        let path = unsafe { CStr::from_ptr(path) }
            .to_string_lossy()
            .into_owned();
        baton.active_textdelta_source_buffer = if let Some(content) =
            baton.base_file_contents.get(&path)
        {
            unsafe {
                svn_stringbuf_ncreate(content.as_ptr().cast::<c_char>(), content.len(), _file_pool)
            }
        } else {
            baton.error = Some(format!("libsvn update had no base content for {path}"));
            unsafe { svn_stringbuf_create_empty(_file_pool) }
        };
        baton.active_file_path = Some(path);
        baton.active_textdelta_buffer = ptr::null_mut();
    }
    unsafe {
        *file_baton = parent_baton;
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn fetch_apply_textdelta(
    file_baton: *mut c_void,
    _base_checksum: *const c_char,
    result_pool: *mut AprPoolT,
    handler: *mut SvnTxdeltaWindowHandlerFunc,
    handler_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    if file_baton.is_null() {
        return ptr::null_mut();
    }
    let baton = unsafe { &mut *(file_baton as *mut FetchEditorUpdateBaton) };
    if baton.active_textdelta_buffer.is_null() {
        baton.active_textdelta_buffer = unsafe { svn_stringbuf_create_empty(result_pool) };
    }
    let source_stream = if baton.active_textdelta_source_buffer.is_null() {
        unsafe { svn_stream_empty(result_pool) }
    } else {
        unsafe { svn_stream_from_stringbuf(baton.active_textdelta_source_buffer, result_pool) }
    };
    let target_stream =
        unsafe { svn_stream_from_stringbuf(baton.active_textdelta_buffer, result_pool) };
    unsafe {
        svn_txdelta_apply(
            source_stream,
            target_stream,
            ptr::null_mut(),
            ptr::null(),
            result_pool,
            handler,
            handler_baton,
        );
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn fetch_change_file_prop(
    file_baton: *mut c_void,
    name: *const c_char,
    value: *const svn_string_t,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if file_baton.is_null() {
        return ptr::null_mut();
    }
    let baton = unsafe { &mut *(file_baton as *mut FetchEditorUpdateBaton) };
    let Some(path) = baton.active_file_path.as_deref() else {
        baton.error = Some("libsvn file property callback had no active file path".to_string());
        return ptr::null_mut();
    };
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
    let value = if value.is_null() {
        None
    } else {
        let bytes = unsafe { slice::from_raw_parts((*value).data.cast::<u8>(), (*value).len) };
        Some(String::from_utf8_lossy(bytes).into_owned())
    };
    if let Err(error) = baton.editor.change_file_prop(path, &name, value.as_deref()) {
        baton.error = Some(error);
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn fetch_change_dir_prop(
    dir_baton: *mut c_void,
    name: *const c_char,
    value: *const svn_string_t,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if dir_baton.is_null() {
        return ptr::null_mut();
    }
    let baton = unsafe { &mut *(dir_baton as *mut FetchEditorUpdateBaton) };
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
    if name.starts_with("svn:entry:") {
        return ptr::null_mut();
    }
    let value = if value.is_null() {
        None
    } else {
        let bytes = unsafe { slice::from_raw_parts((*value).data.cast::<u8>(), (*value).len) };
        Some(String::from_utf8_lossy(bytes).into_owned())
    };
    if let Err(error) = baton.editor.change_directory_prop(
        baton
            .active_directory_paths
            .last()
            .map(String::as_str)
            .unwrap_or_default(),
        &name,
        value.as_deref(),
    ) {
        baton.error = Some(error);
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn fetch_close_file(
    file_baton: *mut c_void,
    _text_checksum: *const c_char,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if file_baton.is_null() {
        return ptr::null_mut();
    }
    let baton = unsafe { &mut *(file_baton as *mut FetchEditorUpdateBaton) };
    if let Some(path) = baton.active_file_path.take()
        && !baton.active_textdelta_buffer.is_null()
    {
        let content = unsafe { stringbuf_bytes(baton.active_textdelta_buffer) };
        if let Err(error) = baton.editor.apply_textdelta(&path, &content) {
            baton.error = Some(error);
        }
    }
    baton.active_textdelta_buffer = ptr::null_mut();
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn fetch_close_edit(
    edit_baton: *mut c_void,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    let baton = unsafe { &mut *(edit_baton as *mut FetchEditorUpdateBaton) };
    if let Err(error) = baton.editor.close_edit() {
        baton.error = Some(error);
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_close_edit(
    edit_baton: *mut c_void,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if edit_baton.is_null() {
        return ptr::null_mut();
    }
    let baton = unsafe { &mut *(edit_baton as *mut RecordingEditorBaton) };
    baton.closed = true;
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_open_root(
    edit_baton: *mut c_void,
    base_revision: c_long,
    _dir_pool: *mut AprPoolT,
    root_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    if root_baton.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        *root_baton = edit_baton;
    }
    if edit_baton.is_null() {
        return ptr::null_mut();
    }
    let baton = unsafe { &mut *(edit_baton as *mut RecordingEditorBaton) };
    baton.root_opened = true;
    baton.root_base_revision = base_revision;
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_close_directory(
    dir_baton: *mut c_void,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if !dir_baton.is_null() {
        let baton = unsafe { &mut *(dir_baton as *mut RecordingEditorBaton) };
        baton.closed_directories += 1;
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_delete_entry(
    path: *const c_char,
    revision: c_long,
    parent_baton: *mut c_void,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if path.is_null() || parent_baton.is_null() {
        return ptr::null_mut();
    }
    let path = unsafe { CStr::from_ptr(path) }.to_string_lossy();
    let baton = unsafe { &mut *(parent_baton as *mut RecordingEditorBaton) };
    baton.deleted_entries.push(format!("{path}@{revision}"));
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_add_directory(
    path: *const c_char,
    parent_baton: *mut c_void,
    _copyfrom_path: *const c_char,
    _copyfrom_revision: c_long,
    _dir_pool: *mut AprPoolT,
    child_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    if path.is_null() || parent_baton.is_null() || child_baton.is_null() {
        return ptr::null_mut();
    }
    let path = unsafe { CStr::from_ptr(path) }
        .to_string_lossy()
        .into_owned();
    let baton = unsafe { &mut *(parent_baton as *mut RecordingEditorBaton) };
    baton.added_directories.push(path);
    unsafe {
        *child_baton = parent_baton;
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_open_directory(
    path: *const c_char,
    parent_baton: *mut c_void,
    base_revision: c_long,
    _dir_pool: *mut AprPoolT,
    child_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    if path.is_null() || parent_baton.is_null() || child_baton.is_null() {
        return ptr::null_mut();
    }
    let path = unsafe { CStr::from_ptr(path) }.to_string_lossy();
    let baton = unsafe { &mut *(parent_baton as *mut RecordingEditorBaton) };
    baton
        .opened_directories
        .push(format!("{path}@{base_revision}"));
    unsafe {
        *child_baton = parent_baton;
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_add_file(
    path: *const c_char,
    parent_baton: *mut c_void,
    _copyfrom_path: *const c_char,
    _copyfrom_revision: c_long,
    _file_pool: *mut AprPoolT,
    file_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    if path.is_null() || parent_baton.is_null() || file_baton.is_null() {
        return ptr::null_mut();
    }
    let path = unsafe { CStr::from_ptr(path) }
        .to_string_lossy()
        .into_owned();
    let baton = unsafe { &mut *(parent_baton as *mut RecordingEditorBaton) };
    baton.added_files.push(path);
    unsafe {
        *file_baton = parent_baton;
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_close_file(
    file_baton: *mut c_void,
    _text_checksum: *const c_char,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if !file_baton.is_null() {
        let baton = unsafe { &mut *(file_baton as *mut RecordingEditorBaton) };
        baton.closed_files += 1;
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_open_file(
    path: *const c_char,
    parent_baton: *mut c_void,
    base_revision: c_long,
    _file_pool: *mut AprPoolT,
    file_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    if path.is_null() || parent_baton.is_null() || file_baton.is_null() {
        return ptr::null_mut();
    }
    let path = unsafe { CStr::from_ptr(path) }.to_string_lossy();
    let baton = unsafe { &mut *(parent_baton as *mut RecordingEditorBaton) };
    baton.opened_files.push(format!("{path}@{base_revision}"));
    unsafe {
        *file_baton = parent_baton;
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_change_file_prop(
    file_baton: *mut c_void,
    name: *const c_char,
    value: *const svn_string_t,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if file_baton.is_null() || name.is_null() {
        return ptr::null_mut();
    }
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
    let value = if value.is_null() {
        String::new()
    } else {
        let bytes = unsafe { slice::from_raw_parts((*value).data.cast::<u8>(), (*value).len) };
        String::from_utf8_lossy(bytes).into_owned()
    };
    let baton = unsafe { &mut *(file_baton as *mut RecordingEditorBaton) };
    baton.file_properties.push(format!("{name}={value}"));
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_change_dir_prop(
    dir_baton: *mut c_void,
    name: *const c_char,
    value: *const svn_string_t,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if dir_baton.is_null() || name.is_null() {
        return ptr::null_mut();
    }
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
    let value = if value.is_null() {
        String::new()
    } else {
        let bytes = unsafe { slice::from_raw_parts((*value).data.cast::<u8>(), (*value).len) };
        String::from_utf8_lossy(bytes).into_owned()
    };
    let baton = unsafe { &mut *(dir_baton as *mut RecordingEditorBaton) };
    baton.directory_properties.push(format!("{name}={value}"));
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_apply_textdelta(
    file_baton: *mut c_void,
    _base_checksum: *const c_char,
    _result_pool: *mut AprPoolT,
    handler: *mut SvnTxdeltaWindowHandlerFunc,
    handler_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    if file_baton.is_null() || handler.is_null() || handler_baton.is_null() {
        return ptr::null_mut();
    }
    let baton = unsafe { &mut *(file_baton as *mut RecordingEditorBaton) };
    baton.apply_textdelta_callbacks += 1;
    unsafe {
        *handler = Some(record_textdelta_window);
        *handler_baton = file_baton;
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_apply_textdelta_to_buffer(
    file_baton: *mut c_void,
    _base_checksum: *const c_char,
    result_pool: *mut AprPoolT,
    handler: *mut SvnTxdeltaWindowHandlerFunc,
    handler_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    if file_baton.is_null() || result_pool.is_null() || handler.is_null() || handler_baton.is_null()
    {
        return ptr::null_mut();
    }
    let baton = unsafe { &mut *(file_baton as *mut RecordingEditorBaton) };
    if baton.textdelta_buffer.is_null() {
        return ptr::null_mut();
    }
    let source_stream = unsafe { svn_stream_empty(result_pool) };
    let target_stream = unsafe { svn_stream_from_stringbuf(baton.textdelta_buffer, result_pool) };
    if source_stream.is_null() || target_stream.is_null() {
        return ptr::null_mut();
    }
    baton.textdelta_applied_to_buffer += 1;
    unsafe {
        svn_txdelta_apply(
            source_stream,
            target_stream,
            ptr::null_mut(),
            ptr::null(),
            result_pool,
            handler,
            handler_baton,
        );
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_apply_textdelta_stream(
    _editor: *const SvnDeltaEditorT,
    _file_baton: *mut c_void,
    _base_checksum: *const c_char,
    open_func: SvnTxdeltaStreamOpenFunc,
    _open_baton: *mut c_void,
    _scratch_pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if open_func.is_none() {
        return ptr::null_mut();
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_open_txdelta_stream(
    txdelta_stream: *mut *mut SvnTxdeltaStreamT,
    _baton: *mut c_void,
    _result_pool: *mut AprPoolT,
    _scratch_pool: *mut AprPoolT,
) -> *mut svn_error_t {
    unsafe {
        *txdelta_stream = ptr::null_mut();
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_absent_directory(
    _path: *const c_char,
    parent_baton: *mut c_void,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if !parent_baton.is_null() {
        let baton = unsafe { &mut *(parent_baton as *mut RecordingEditorBaton) };
        baton.absent_directories += 1;
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_absent_file(
    _path: *const c_char,
    parent_baton: *mut c_void,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if !parent_baton.is_null() {
        let baton = unsafe { &mut *(parent_baton as *mut RecordingEditorBaton) };
        baton.absent_files += 1;
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_abort_edit(
    edit_baton: *mut c_void,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if !edit_baton.is_null() {
        let baton = unsafe { &mut *(edit_baton as *mut RecordingEditorBaton) };
        baton.aborted_edits += 1;
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn record_textdelta_window(
    window: *mut SvnTxdeltaWindowT,
    baton: *mut c_void,
) -> *mut svn_error_t {
    if !baton.is_null() {
        let baton = unsafe { &mut *(baton as *mut RecordingEditorBaton) };
        if window.is_null() {
            baton.textdelta_done_windows += 1;
        } else {
            baton.textdelta_windows += 1;
        }
    }
    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
fn create_minimal_svn_repository() -> Result<(tempfile::TempDir, String), String> {
    if !command_succeeds("svnadmin", &["--version"]) || !command_succeeds("svn", &["--version"]) {
        return Err("svnadmin and svn are required".to_string());
    }

    let tmp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let repo = tmp.path().join("repo");
    let wc = tmp.path().join("wc");
    run(tmp.path(), "svnadmin", &["create", path_arg(&repo)?])?;
    let repo_url = url::Url::from_directory_path(repo.canonicalize().map_err(|e| e.to_string())?)
        .map_err(|()| "failed to build file URL for SVN repository".to_string())?
        .to_string()
        .trim_end_matches('/')
        .to_string();
    run(tmp.path(), "svn", &["checkout", &repo_url, "wc"])?;
    run(&wc, "svn", &["mkdir", "trunk"])?;
    run(&wc, "svn", &["commit", "-m", "create trunk"])?;
    let trunk = wc.join("trunk");
    run(&wc, "svn", &["update", "trunk"])?;
    fs::write(trunk.join("file.txt"), b"hello\n").map_err(|error| error.to_string())?;
    run(&wc, "svn", &["add", "trunk/file.txt"])?;
    run(&wc, "svn", &["commit", "-m", "add file"])?;
    fs::write(trunk.join("file.txt"), b"hello again\n").map_err(|error| error.to_string())?;
    run(&wc, "svn", &["commit", "-m", "modify file"])?;
    run(
        &wc,
        "svn",
        &["propset", "svn:needs-lock", "x", "trunk/file.txt"],
    )?;
    run(&wc, "svn", &["commit", "-m", "set needs lock"])?;
    run(&wc, "svn", &["update", "trunk"])?;
    run(&wc, "svn", &["propset", "svn:ignore", "target", "trunk"])?;
    run(&wc, "svn", &["commit", "-m", "set directory ignore"])?;
    run(&wc, "svn", &["delete", "trunk/file.txt"])?;
    run(&wc, "svn", &["commit", "-m", "delete file"])?;
    run(&wc, "svn", &["mkdir", "trunk/subdir"])?;
    run(&wc, "svn", &["commit", "-m", "add subdir"])?;
    fs::write(trunk.join("subdir").join("nested.txt"), b"nested\n")
        .map_err(|error| error.to_string())?;
    run(&wc, "svn", &["add", "trunk/subdir/nested.txt"])?;
    run(&wc, "svn", &["commit", "-m", "add nested file"])?;
    run(&wc, "svn", &["update", "trunk/subdir"])?;
    run(
        &wc,
        "svn",
        &["propset", "svn:ignore", "nested-target", "trunk/subdir"],
    )?;
    run(&wc, "svn", &["commit", "-m", "set nested directory ignore"])?;

    Ok((tmp, repo_url))
}

#[cfg(git_svn_rs_libsvn_linked)]
fn command_succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(git_svn_rs_libsvn_linked)]
fn run(cwd: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|error| format!("{program} failed to start: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(format!(
        "{program} failed with status {}: {}{}",
        output.status,
        stderr.trim(),
        if stdout.trim().is_empty() {
            String::new()
        } else {
            format!(" stdout: {}", stdout.trim())
        }
    ))
}

#[cfg(git_svn_rs_libsvn_linked)]
fn path_arg(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()))
}
