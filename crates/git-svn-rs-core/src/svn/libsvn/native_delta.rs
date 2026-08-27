#![cfg_attr(
    all(not(windows), target_pointer_width = "64"),
    allow(clippy::unnecessary_fallible_conversions)
)]

use crate::path_url::add_path_to_url;
use crate::svn::editor::FetchEditor;
use crate::svn::ra::{RaSession, SvnNodeKind, UpdateRequest};
use crate::svn::{ChangeAction, NodeKind};
use md5::{Digest, Md5};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_long, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::{ptr, slice};

use super::{
    AprPoolT, LibSvnBackend, SVN_DEPTH_INFINITY, SvnDeltaEditorT, SvnRaReporter3T,
    SvnTxdeltaStreamOpenFunc, SvnTxdeltaWindowHandlerFunc, callback_error_message,
    editor_copy_from, editor_path, get_file, remapped_editor_path, stringbuf_bytes, svn_call,
    svn_delta_default_editor, svn_error_t, svn_ra_do_switch3, svn_ra_do_update3, svn_ra_reparent,
    svn_stream_empty, svn_stream_from_stringbuf, svn_string_t, svn_stringbuf_create_empty,
    svn_stringbuf_ncreate, svn_stringbuf_t, svn_txdelta_apply, svn_txdelta_next_window,
};
#[cfg(test)]
use super::{AprRuntime, svn_error_clear, svn_error_detail};

struct FileBaton {
    update_baton: *mut c_void,
    path: String,
    source: *mut svn_stringbuf_t,
    target: *mut svn_stringbuf_t,
    source_bytes: Vec<u8>,
}

struct UpdateBaton<'a> {
    editor: &'a mut dyn FetchEditor,
    directories: Vec<String>,
    active_files: BTreeSet<usize>,
    added_file_properties: BTreeMap<String, BTreeMap<String, Vec<u8>>>,
    base_contents: BTreeMap<String, Vec<u8>>,
    copy_sources: BTreeMap<String, (String, u32)>,
    copy_contents: BTreeMap<String, Vec<u8>>,
    path_prefix: String,
    strip_reporter_prefix: Option<String>,
    error: Option<String>,
}

impl Drop for UpdateBaton<'_> {
    fn drop(&mut self) {
        for address in std::mem::take(&mut self.active_files) {
            unsafe { drop(Box::from_raw(address as *mut FileBaton)) };
        }
    }
}

impl UpdateBaton<'_> {
    fn fail(&mut self, error: impl Into<String>) {
        if self.error.is_none() {
            self.error = Some(error.into());
        }
    }

    fn invoke(&mut self, callback: impl FnOnce(&mut dyn FetchEditor) -> Result<(), String>) {
        if self.error.is_some() {
            return;
        }
        match catch_unwind(AssertUnwindSafe(|| callback(self.editor))) {
            Ok(Ok(())) => {}
            Ok(Err(error)) => self.fail(error),
            Err(_) => self.fail("FetchEditor callback panicked during native SVN update"),
        }
    }

    fn map_path(&self, path: &str) -> String {
        let path = editor_path(path);
        let path = self
            .strip_reporter_prefix
            .as_deref()
            .and_then(|prefix| {
                if path == prefix {
                    Some("")
                } else {
                    path.strip_prefix(&format!("{prefix}/"))
                }
            })
            .unwrap_or(&path);
        if self.path_prefix.is_empty()
            || path == self.path_prefix
            || path.starts_with(&format!("{}/", self.path_prefix))
        {
            path.to_string()
        } else if path.is_empty() {
            self.path_prefix.clone()
        } else {
            format!("{}/{path}", self.path_prefix)
        }
    }
}

pub(super) fn drive_update(
    backend: &LibSvnBackend,
    target: &str,
    request: UpdateRequest,
    fetch_editor: &mut dyn FetchEditor,
) -> Result<(), String> {
    drive_report(backend, target, request, None, target, fetch_editor)
}

pub(super) fn drive_switch(
    backend: &LibSvnBackend,
    target: &str,
    request: UpdateRequest,
    switch_url: &str,
    source_path: &str,
    fetch_editor: &mut dyn FetchEditor,
) -> Result<(), String> {
    drive_report(
        backend,
        target,
        request,
        Some(switch_url),
        source_path,
        fetch_editor,
    )
}

fn drive_report(
    backend: &LibSvnBackend,
    target: &str,
    request: UpdateRequest,
    switch_url: Option<&str>,
    source_path: &str,
    fetch_editor: &mut dyn FetchEditor,
) -> Result<(), String> {
    let target_revision: c_long = request.target_revision.try_into().map_err(|_| {
        format!(
            "SVN revision {} does not fit in svn_revnum_t",
            request.target_revision
        )
    })?;
    let base_revision = request.base_revision.unwrap_or(0);
    let base_revision_raw: c_long = base_revision
        .try_into()
        .map_err(|_| format!("SVN revision {base_revision} does not fit in svn_revnum_t"))?;
    let start_empty = i32::from(request.base_revision.is_none());
    let repository_root_url = backend.repos_root().to_string();
    let session_path = backend.session_repository_path();

    let mut base_paths = BTreeSet::new();
    let mut copy_sources = BTreeMap::new();
    let mut copy_file_targets = BTreeSet::new();
    let mut added_file_properties = BTreeMap::new();
    for revision in backend.get_log(
        &[source_path],
        request.target_revision,
        request.target_revision,
    )? {
        for change in revision.changed_paths {
            let target_path = remapped_editor_path(&change.path, source_path, target);
            if matches!(change.action, ChangeAction::Add | ChangeAction::Replace)
                && matches!(change.kind, NodeKind::File | NodeKind::Symlink)
                && !change.properties.is_empty()
            {
                added_file_properties.insert(
                    target_path.clone(),
                    change
                        .properties
                        .iter()
                        .map(|(name, value)| (name.clone(), value.as_bytes().to_vec()))
                        .collect(),
                );
            }
            if let Some(copy_from) = editor_copy_from(&change.copy_from_path, change.copy_from_rev)
            {
                if matches!(change.kind, NodeKind::File | NodeKind::Symlink) {
                    copy_file_targets.insert(target_path.clone());
                }
                copy_sources.insert(target_path, copy_from);
            }
        }
    }
    if let Some(base_revision) = request.base_revision {
        let start = base_revision.saturating_add(1);
        if start <= request.target_revision {
            for revision in backend.get_log(&[target], start, request.target_revision)? {
                for change in revision.changed_paths {
                    if matches!(change.kind, NodeKind::File | NodeKind::Symlink)
                        && !matches!(change.action, ChangeAction::Add)
                        && backend.check_path(&change.path, base_revision)?
                            == Some(SvnNodeKind::File)
                    {
                        base_paths.insert(change.path);
                    }
                }
            }
        }
    }

    backend.with_session(|session, pool| unsafe {
        let repository_root_url = CString::new(repository_root_url.as_str())
            .map_err(|_| "SVN repository root URL contains NUL".to_string())?;
        svn_call(
            svn_ra_reparent(session, repository_root_url.as_ptr(), pool),
            "svn_ra_reparent(repository root)",
        )?;
        let mut base_contents = BTreeMap::new();
        for path in base_paths {
            let repository_path = if session_path.is_empty() {
                path.clone()
            } else {
                format!(
                    "{}/{}",
                    session_path.trim_matches('/'),
                    path.trim_matches('/')
                )
            };
            let (content, _) = get_file(session, pool, &repository_path, base_revision_raw)?;
            base_contents.insert(editor_path(&path), content);
        }
        let mut copy_contents = BTreeMap::new();
        for target_path in &copy_file_targets {
            let Some((source_path, declared_revision)) = copy_sources.get(target_path).cloned()
            else {
                continue;
            };
            let mut candidates = vec![declared_revision];
            candidates.extend((declared_revision.saturating_add(1)..request.target_revision).rev());
            let mut resolved = None;
            let mut last_error = None;
            for candidate in candidates {
                let candidate_raw: c_long = candidate.try_into().map_err(|_| {
                    format!("SVN copy source revision {candidate} does not fit in svn_revnum_t")
                })?;
                match get_file(session, pool, &source_path, candidate_raw) {
                    Ok((content, _)) => {
                        resolved = Some((candidate, content));
                        break;
                    }
                    Err(error) => last_error = Some(error),
                }
            }
            let Some((resolved_revision, content)) = resolved else {
                return Err(format!(
                    "{} while reading copy source {source_path}@{declared_revision} from {}",
                    last_error.unwrap_or_else(|| "SVN copy source was not found".to_string()),
                    backend.repos_root()
                ));
            };
            copy_sources.insert(target_path.clone(), (source_path, resolved_revision));
            copy_contents.insert(target_path.clone(), content);
        }

        let target = target.trim_matches('/');
        let session_path = session_path.trim_matches('/');
        let session_root_update = target.is_empty() && !session_path.is_empty();
        let reporter_target = if session_root_update {
            session_path
        } else {
            target
        };
        let (reporter_parent, update_target) = reporter_target
            .rsplit_once('/')
            .map_or(("", reporter_target), |(parent, name)| (parent, name));
        let path_prefix = if session_root_update {
            ""
        } else {
            reporter_parent
        };
        let session_url = if session_root_update {
            add_path_to_url(backend.repos_root(), reporter_parent)
        } else {
            add_path_to_url(backend.url(), path_prefix)
        };
        let session_url =
            CString::new(session_url).map_err(|_| "SVN reparent URL contains NUL".to_string())?;
        svn_call(
            svn_ra_reparent(session, session_url.as_ptr(), pool),
            "svn_ra_reparent(update target)",
        )?;

        let editor = svn_delta_default_editor(pool);
        if editor.is_null() {
            return Err("svn_delta_default_editor returned null".to_string());
        }

        let mut baton = UpdateBaton {
            editor: fetch_editor,
            directories: vec![target.to_string()],
            active_files: BTreeSet::new(),
            added_file_properties,
            base_contents,
            copy_sources,
            copy_contents,
            path_prefix: path_prefix.to_string(),
            strip_reporter_prefix: session_root_update.then(|| reporter_target.to_string()),
            error: None,
        };
        (*editor).set_target_revision = Some(set_target_revision);
        (*editor).open_root = Some(open_root);
        (*editor).delete_entry = Some(delete_entry);
        (*editor).add_directory = Some(add_directory);
        (*editor).open_directory = Some(open_directory);
        (*editor).close_directory = Some(close_directory);
        (*editor).add_file = Some(add_file);
        (*editor).open_file = Some(open_file);
        (*editor).apply_textdelta = Some(apply_textdelta);
        (*editor).apply_textdelta_stream = Some(apply_textdelta_stream);
        (*editor).change_dir_prop = Some(change_dir_prop);
        (*editor).change_file_prop = Some(change_file_prop);
        (*editor).close_file = Some(close_file);
        (*editor).absent_directory = Some(absent_directory);
        (*editor).absent_file = Some(absent_file);
        (*editor).close_edit = Some(close_edit);
        (*editor).abort_edit = Some(abort_edit);

        let target = CString::new(update_target)
            .map_err(|_| "SVN update target contains NUL".to_string())?;
        let mut reporter: *const SvnRaReporter3T = ptr::null();
        let mut report_baton: *mut c_void = ptr::null_mut();
        let operation_baton = (&mut baton as *mut UpdateBaton).cast::<c_void>();
        if let Some(switch_url) = switch_url {
            let switch_url =
                CString::new(switch_url).map_err(|_| "SVN switch URL contains NUL".to_string())?;
            svn_call(
                svn_ra_do_switch3(
                    session,
                    &mut reporter,
                    &mut report_baton,
                    target_revision,
                    target.as_ptr(),
                    SVN_DEPTH_INFINITY,
                    switch_url.as_ptr(),
                    1,
                    0,
                    editor,
                    operation_baton,
                    pool,
                    pool,
                ),
                "svn_ra_do_switch3",
            )?;
        } else {
            svn_call(
                svn_ra_do_update3(
                    session,
                    &mut reporter,
                    &mut report_baton,
                    target_revision,
                    target.as_ptr(),
                    SVN_DEPTH_INFINITY,
                    1,
                    0,
                    editor,
                    operation_baton,
                    pool,
                    pool,
                ),
                "svn_ra_do_update3",
            )?;
        }
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
                base_revision_raw,
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
        baton.error.take().map_or(Ok(()), Err)
    })
}

// The operation baton lives on `drive_report`'s stack until the synchronous
// reporter finishes. Each callback borrows it only for that callback invocation.
unsafe fn baton<'borrow, 'editor>(raw: *mut c_void) -> Option<&'borrow mut UpdateBaton<'editor>> {
    if raw.is_null() {
        None
    } else {
        Some(unsafe { &mut *raw.cast::<UpdateBaton>() })
    }
}

unsafe fn callback_error(baton: &UpdateBaton<'_>) -> *mut svn_error_t {
    let Some(error) = baton.error.as_deref() else {
        return ptr::null_mut();
    };
    unsafe { callback_error_message(error) }
}

unsafe fn path(raw: *const c_char) -> Option<String> {
    (!raw.is_null()).then(|| {
        unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned()
    })
}

unsafe extern "C" fn set_target_revision(
    edit_baton: *mut c_void,
    revision: c_long,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    let Some(baton) = (unsafe { baton(edit_baton) }) else {
        return unsafe { callback_error_message("libsvn target-revision callback had no baton") };
    };
    match u32::try_from(revision) {
        Ok(revision) => baton.invoke(|editor| editor.open_root(revision)),
        Err(_) => baton.fail(format!(
            "SVN target revision {revision} does not fit in u32"
        )),
    }
    unsafe { callback_error(baton) }
}

unsafe extern "C" fn open_root(
    edit_baton: *mut c_void,
    _base_revision: c_long,
    _pool: *mut AprPoolT,
    root_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    if edit_baton.is_null() || root_baton.is_null() {
        return unsafe {
            callback_error_message("libsvn open-root callback had null baton or output")
        };
    }
    unsafe { *root_baton = edit_baton };
    ptr::null_mut()
}

unsafe extern "C" fn delete_entry(
    raw_path: *const c_char,
    revision: c_long,
    parent_baton: *mut c_void,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    let Some(baton) = (unsafe { baton(parent_baton) }) else {
        return unsafe { callback_error_message("libsvn delete callback had no baton") };
    };
    let Some(path) = (unsafe { path(raw_path) }) else {
        baton.fail("libsvn delete callback had a null path");
        return unsafe { callback_error(baton) };
    };
    let path = baton.map_path(&path);
    match u32::try_from(revision) {
        Ok(revision) => baton.invoke(|editor| editor.delete_entry(&path, revision)),
        Err(_) => baton.fail(format!(
            "SVN delete revision {revision} does not fit in u32"
        )),
    }
    unsafe { callback_error(baton) }
}

unsafe extern "C" fn add_directory(
    raw_path: *const c_char,
    parent_baton: *mut c_void,
    _copy_path: *const c_char,
    _copy_revision: c_long,
    _pool: *mut AprPoolT,
    child_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    if parent_baton.is_null() || child_baton.is_null() {
        return unsafe {
            callback_error_message("libsvn add-directory callback had null baton or output")
        };
    }
    unsafe { *child_baton = parent_baton };
    let Some(baton) = (unsafe { baton(parent_baton) }) else {
        return unsafe { callback_error_message("libsvn add-directory callback had no baton") };
    };
    let Some(path) = (unsafe { path(raw_path) }) else {
        baton.fail("libsvn add-directory callback had a null path");
        return unsafe { callback_error(baton) };
    };
    let path = baton.map_path(&path);
    let copy = baton.copy_sources.get(&path).cloned();
    baton
        .invoke(|editor| editor.add_directory(&path, copy.as_ref().map(|(p, r)| (p.as_str(), *r))));
    baton.directories.push(path);
    unsafe { callback_error(baton) }
}

unsafe extern "C" fn open_directory(
    raw_path: *const c_char,
    parent_baton: *mut c_void,
    _base_revision: c_long,
    _pool: *mut AprPoolT,
    child_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    if parent_baton.is_null() || child_baton.is_null() {
        return unsafe {
            callback_error_message("libsvn open-directory callback had null baton or output")
        };
    }
    unsafe { *child_baton = parent_baton };
    if let Some(baton) = unsafe { baton(parent_baton) } {
        if let Some(path) = unsafe { path(raw_path) } {
            let path = baton.map_path(&path);
            baton.directories.push(path);
        } else {
            baton.fail("libsvn open-directory callback had a null path");
        }
        return unsafe { callback_error(baton) };
    }
    unsafe { callback_error_message("libsvn open-directory callback had no baton") }
}

unsafe extern "C" fn close_directory(
    dir_baton: *mut c_void,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if let Some(baton) = unsafe { baton(dir_baton) } {
        if baton.directories.pop().is_none() {
            baton.fail("libsvn close-directory callback had no active directory");
        }
        return unsafe { callback_error(baton) };
    }
    unsafe { callback_error_message("libsvn close-directory callback had no baton") }
}

unsafe extern "C" fn add_file(
    raw_path: *const c_char,
    parent_baton: *mut c_void,
    _copy_path: *const c_char,
    _copy_revision: c_long,
    pool: *mut AprPoolT,
    file_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    if parent_baton.is_null() || file_baton.is_null() || pool.is_null() {
        return unsafe {
            callback_error_message("libsvn add-file callback had null baton, output, or pool")
        };
    }
    unsafe { *file_baton = ptr::null_mut() };
    let Some(baton) = (unsafe { baton(parent_baton) }) else {
        return unsafe { callback_error_message("libsvn add-file callback had no baton") };
    };
    let Some(path) = (unsafe { path(raw_path) }) else {
        baton.fail("libsvn add-file callback had a null path");
        return unsafe { callback_error(baton) };
    };
    let path = baton.map_path(&path);
    let copy = baton.copy_sources.get(&path).cloned();
    let source_bytes = copy
        .as_ref()
        .and_then(|_| baton.copy_contents.get(&path))
        .cloned()
        .unwrap_or_default();
    if let Some((source_path, source_revision)) = copy.as_ref() {
        baton.invoke(|editor| {
            editor.add_file_with_copy_content(
                &path,
                (source_path.as_str(), *source_revision),
                &source_bytes,
            )
        });
    } else {
        baton.invoke(|editor| editor.add_file(&path, None));
    }
    if let Some(properties) = baton.added_file_properties.get(&path).cloned() {
        for (name, value) in properties {
            baton.invoke(|editor| editor.change_file_prop_bytes(&path, &name, Some(&value)));
        }
    }
    let source = if copy.is_some() {
        unsafe {
            svn_stringbuf_ncreate(
                source_bytes.as_ptr().cast::<c_char>(),
                source_bytes.len(),
                pool,
            )
        }
    } else {
        ptr::null_mut()
    };
    if copy.is_some() && source.is_null() {
        baton.fail("libsvn failed to allocate copy-source buffer");
    }
    let mut file = Box::new(FileBaton {
        update_baton: parent_baton,
        path,
        source,
        target: ptr::null_mut(),
        source_bytes,
    });
    let file_pointer = (&mut *file as *mut FileBaton).cast::<c_void>();
    baton.active_files.insert(file_pointer as usize);
    unsafe { *file_baton = Box::into_raw(file).cast::<c_void>() };
    unsafe { callback_error(baton) }
}

unsafe extern "C" fn open_file(
    raw_path: *const c_char,
    parent_baton: *mut c_void,
    _base_revision: c_long,
    pool: *mut AprPoolT,
    file_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    if parent_baton.is_null() || file_baton.is_null() || pool.is_null() {
        return unsafe {
            callback_error_message("libsvn open-file callback had null baton, output, or pool")
        };
    }
    unsafe { *file_baton = ptr::null_mut() };
    let Some(baton) = (unsafe { baton(parent_baton) }) else {
        return unsafe { callback_error_message("libsvn open-file callback had no baton") };
    };
    let Some(path) = (unsafe { path(raw_path) }) else {
        baton.fail("libsvn open-file callback had a null path");
        return unsafe { callback_error(baton) };
    };
    let path = baton.map_path(&path);
    let source_bytes = baton.base_contents.get(&path).cloned().unwrap_or_default();
    let source = baton.base_contents.get(&path).map_or_else(
        || unsafe { svn_stringbuf_create_empty(pool) },
        |content| unsafe {
            svn_stringbuf_ncreate(content.as_ptr().cast::<c_char>(), content.len(), pool)
        },
    );
    if !baton.base_contents.contains_key(&path) {
        baton.fail(format!("libsvn update had no base content for {path}"));
    }
    if source.is_null() {
        baton.fail(format!("libsvn failed to allocate base buffer for {path}"));
    }
    let mut file = Box::new(FileBaton {
        update_baton: parent_baton,
        path,
        source,
        target: ptr::null_mut(),
        source_bytes,
    });
    let file_pointer = (&mut *file as *mut FileBaton).cast::<c_void>();
    baton.active_files.insert(file_pointer as usize);
    unsafe { *file_baton = Box::into_raw(file).cast::<c_void>() };
    unsafe { callback_error(baton) }
}

unsafe extern "C" fn apply_textdelta(
    file_baton: *mut c_void,
    base_checksum: *const c_char,
    pool: *mut AprPoolT,
    handler: *mut SvnTxdeltaWindowHandlerFunc,
    handler_baton: *mut *mut c_void,
) -> *mut svn_error_t {
    let Some(file) = (unsafe { (file_baton as *mut FileBaton).as_mut() }) else {
        return unsafe {
            callback_error_message("libsvn apply-textdelta callback had no file baton")
        };
    };
    let Some(baton) = (unsafe { baton(file.update_baton) }) else {
        return unsafe {
            callback_error_message("libsvn apply-textdelta callback had no update baton")
        };
    };
    if handler.is_null() || handler_baton.is_null() || pool.is_null() {
        baton.fail("libsvn apply-textdelta callback had null output or pool");
        return unsafe { callback_error(baton) };
    }
    if !baton.active_files.contains(&(file_baton as usize)) {
        baton.fail("libsvn apply-textdelta callback had an inactive file baton");
        return unsafe { callback_error(baton) };
    }
    let checksum_error = unsafe { path(base_checksum) }
        .and_then(|expected| validate_md5("base", &expected, &file.source_bytes).err());
    if let Some(error) = checksum_error {
        baton.fail(error);
    }
    if file.target.is_null() {
        file.target = unsafe { svn_stringbuf_create_empty(pool) };
    }
    if file.target.is_null() {
        baton.fail("libsvn failed to allocate textdelta target buffer");
        return unsafe { callback_error(baton) };
    }
    let source = if file.source.is_null() {
        unsafe { svn_stream_empty(pool) }
    } else {
        unsafe { svn_stream_from_stringbuf(file.source, pool) }
    };
    let target = unsafe { svn_stream_from_stringbuf(file.target, pool) };
    if source.is_null() || target.is_null() {
        baton.fail("libsvn failed to allocate textdelta streams");
        return unsafe { callback_error(baton) };
    }
    unsafe {
        svn_txdelta_apply(
            source,
            target,
            ptr::null_mut(),
            ptr::null(),
            pool,
            handler,
            handler_baton,
        );
    }
    if unsafe { (*handler).is_none() } || unsafe { (*handler_baton).is_null() } {
        baton.fail("libsvn textdelta apply returned a null handler or baton");
    }
    unsafe { callback_error(baton) }
}

unsafe extern "C" fn apply_textdelta_stream(
    _editor: *const SvnDeltaEditorT,
    file_baton: *mut c_void,
    base_checksum: *const c_char,
    open_func: SvnTxdeltaStreamOpenFunc,
    open_baton: *mut c_void,
    scratch_pool: *mut AprPoolT,
) -> *mut svn_error_t {
    let Some(open_func) = open_func else {
        return unsafe { callback_error_message("libsvn textdelta stream had no open callback") };
    };
    if file_baton.is_null() || scratch_pool.is_null() {
        return unsafe { callback_error_message("libsvn textdelta stream had null baton or pool") };
    }

    let mut stream = ptr::null_mut();
    let error = unsafe { open_func(&mut stream, open_baton, scratch_pool, scratch_pool) };
    if !error.is_null() {
        return error;
    }
    if stream.is_null() {
        return unsafe { callback_error_message("libsvn textdelta stream open returned null") };
    }

    let mut handler = None;
    let mut handler_baton = ptr::null_mut();
    let error = unsafe {
        apply_textdelta(
            file_baton,
            base_checksum,
            scratch_pool,
            &mut handler,
            &mut handler_baton,
        )
    };
    if !error.is_null() {
        return error;
    }
    let Some(handler) = handler else {
        return unsafe { callback_error_message("libsvn textdelta stream had no window handler") };
    };

    loop {
        let mut window = ptr::null_mut();
        let error = unsafe { svn_txdelta_next_window(&mut window, stream, scratch_pool) };
        if !error.is_null() {
            return error;
        }
        let error = unsafe { handler(window, handler_baton) };
        if !error.is_null() {
            return error;
        }
        if window.is_null() {
            return ptr::null_mut();
        }
    }
}

unsafe extern "C" fn change_file_prop(
    file_baton: *mut c_void,
    raw_name: *const c_char,
    value: *const svn_string_t,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    let Some(file) = (unsafe { (file_baton as *mut FileBaton).as_mut() }) else {
        return unsafe {
            callback_error_message("libsvn file-property callback had no file baton")
        };
    };
    let Some(baton) = (unsafe { baton(file.update_baton) }) else {
        return unsafe {
            callback_error_message("libsvn file-property callback had no update baton")
        };
    };
    let Some(name) = (unsafe { path(raw_name) }) else {
        baton.fail("libsvn file-property callback had a null name");
        return unsafe { callback_error(baton) };
    };
    if name.starts_with("svn:entry:") {
        return ptr::null_mut();
    }
    if !baton.active_files.contains(&(file_baton as usize)) {
        baton.fail("libsvn file-property callback had an inactive file baton");
        return unsafe { callback_error(baton) };
    }
    let path = file.path.clone();
    let value = if value.is_null() {
        None
    } else {
        match unsafe { svn_string_bytes(value) } {
            Ok(value) => Some(value),
            Err(error) => {
                baton.fail(error);
                return unsafe { callback_error(baton) };
            }
        }
    };
    baton.invoke(|editor| editor.change_file_prop_bytes(&path, &name, value.as_deref()));
    unsafe { callback_error(baton) }
}

unsafe extern "C" fn change_dir_prop(
    dir_baton: *mut c_void,
    raw_name: *const c_char,
    value: *const svn_string_t,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    let Some(baton) = (unsafe { baton(dir_baton) }) else {
        return unsafe {
            callback_error_message("libsvn directory-property callback had no baton")
        };
    };
    let Some(name) = (unsafe { path(raw_name) }) else {
        baton.fail("libsvn directory-property callback had a null name");
        return unsafe { callback_error(baton) };
    };
    if name.starts_with("svn:entry:") {
        return ptr::null_mut();
    }
    let Some(path) = baton.directories.last().cloned() else {
        baton.fail("libsvn directory-property callback had no active directory");
        return unsafe { callback_error(baton) };
    };
    let value = if value.is_null() {
        None
    } else {
        match unsafe { svn_string_bytes(value) } {
            Ok(value) => Some(value),
            Err(error) => {
                baton.fail(error);
                return unsafe { callback_error(baton) };
            }
        }
    };
    baton.invoke(|editor| editor.change_directory_prop_bytes(&path, &name, value.as_deref()));
    unsafe { callback_error(baton) }
}

unsafe fn svn_string_bytes(value: *const svn_string_t) -> Result<Vec<u8>, String> {
    if value.is_null() {
        return Err("libsvn property callback had a null string".to_string());
    }
    let value = unsafe { &*value };
    if value.len == 0 {
        Ok(Vec::new())
    } else if value.data.is_null() {
        Err("libsvn property string had nonzero length and null data".to_string())
    } else {
        Ok(unsafe { slice::from_raw_parts(value.data.cast::<u8>(), value.len) }.to_vec())
    }
}

unsafe extern "C" fn close_file(
    file_baton: *mut c_void,
    checksum: *const c_char,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    let Some(file) = (unsafe { (file_baton as *mut FileBaton).as_mut() }) else {
        return unsafe { callback_error_message("libsvn close-file callback had no baton") };
    };
    let Some(baton) = (unsafe { baton(file.update_baton) }) else {
        return unsafe { callback_error_message("libsvn close-file callback had no update baton") };
    };
    if !baton.active_files.contains(&(file_baton as usize)) {
        baton.fail("libsvn close-file callback had an inactive file baton");
        return unsafe { callback_error(baton) };
    }
    let content = if file.target.is_null() {
        file.source_bytes.clone()
    } else {
        unsafe { stringbuf_bytes(file.target) }
    };
    if let Some(expected) = unsafe { path(checksum) }
        && let Err(error) = validate_md5("result", &expected, &content)
    {
        baton.fail(error);
    }
    if !file.target.is_null() {
        let file_path = file.path.clone();
        baton.invoke(|editor| editor.apply_textdelta(&file_path, &content));
    }
    let error = unsafe { callback_error(baton) };
    if baton.active_files.remove(&(file_baton as usize)) {
        unsafe { drop(Box::from_raw(file_baton as *mut FileBaton)) };
    }
    error
}

fn validate_md5(kind: &str, expected: &str, content: &[u8]) -> Result<(), String> {
    let actual = format!("{:x}", Md5::digest(content));
    if expected.eq_ignore_ascii_case(&actual) {
        Ok(())
    } else {
        Err(format!(
            "SVN {kind} checksum mismatch: expected {expected}, calculated {actual}"
        ))
    }
}

unsafe extern "C" fn close_edit(edit_baton: *mut c_void, _pool: *mut AprPoolT) -> *mut svn_error_t {
    if let Some(baton) = unsafe { baton(edit_baton) } {
        baton.invoke(|editor| editor.close_edit());
        return unsafe { callback_error(baton) };
    }
    unsafe { callback_error_message("libsvn close-edit callback had no baton") }
}

unsafe extern "C" fn absent_directory(
    raw_path: *const c_char,
    parent_baton: *mut c_void,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    let Some(baton) = (unsafe { baton(parent_baton) }) else {
        return unsafe { callback_error_message("libsvn absent-directory callback had no baton") };
    };
    let Some(path) = (unsafe { path(raw_path) }) else {
        baton.fail("libsvn absent-directory callback had a null path");
        return unsafe { callback_error(baton) };
    };
    let path = baton.map_path(&path);
    baton.invoke(|editor| editor.absent_directory(&path));
    unsafe { callback_error(baton) }
}

unsafe extern "C" fn absent_file(
    raw_path: *const c_char,
    parent_baton: *mut c_void,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    let Some(baton) = (unsafe { baton(parent_baton) }) else {
        return unsafe { callback_error_message("libsvn absent-file callback had no baton") };
    };
    let Some(path) = (unsafe { path(raw_path) }) else {
        baton.fail("libsvn absent-file callback had a null path");
        return unsafe { callback_error(baton) };
    };
    let path = baton.map_path(&path);
    baton.invoke(|editor| editor.absent_file(&path));
    unsafe { callback_error(baton) }
}

unsafe extern "C" fn abort_edit(edit_baton: *mut c_void, _pool: *mut AprPoolT) -> *mut svn_error_t {
    if let Some(baton) = unsafe { baton(edit_baton) } {
        baton.invoke(|editor| editor.abort_edit());
        baton.fail("libsvn aborted the native update edit");
        return unsafe { callback_error(baton) };
    }
    unsafe { callback_error_message("libsvn abort-edit callback had no baton") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_md5_and_reports_mismatch() {
        validate_md5("result", "5d41402abc4b2a76b9719d911017c592", b"hello").unwrap();
        let error = validate_md5("base", "00000000000000000000000000000000", b"hello").unwrap_err();
        assert!(error.contains("base checksum mismatch"), "{error}");
        assert!(
            error.contains("5d41402abc4b2a76b9719d911017c592"),
            "{error}"
        );
    }

    #[test]
    fn native_callbacks_return_owned_errors_for_null_required_inputs() {
        AprRuntime::initialize().unwrap();
        let _pool = AprRuntime.create_pool().unwrap();
        let error = unsafe { open_root(ptr::null_mut(), 0, ptr::null_mut(), ptr::null_mut()) };
        assert!(!error.is_null());
        let detail = unsafe { svn_error_detail(error, "open_root") };
        unsafe { svn_error_clear(error) };
        assert!(detail.contains("null baton or output"), "{detail}");

        let error = unsafe { close_file(ptr::null_mut(), ptr::null(), ptr::null_mut()) };
        assert!(!error.is_null());
        let detail = unsafe { svn_error_detail(error, "close_file") };
        unsafe { svn_error_clear(error) };
        assert!(detail.contains("no baton"), "{detail}");
    }

    #[test]
    fn property_string_rejects_nonzero_length_with_null_data() {
        let value = svn_string_t {
            data: ptr::null(),
            len: 1,
        };

        let error = unsafe { svn_string_bytes(&value) }.unwrap_err();

        assert!(error.contains("nonzero length and null data"), "{error}");
    }
}
