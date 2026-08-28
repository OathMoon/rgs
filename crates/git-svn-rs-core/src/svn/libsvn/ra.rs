use super::LibSvnBackend;
use super::ffi::*;
use super::runtime::{AprRuntime, callback_error_message, svn_call};
use crate::svn::ra::{RaSession, SvnNodeKind};
use crate::svn::{ChangeAction, ChangedPath, NodeKind, RevisionEvent};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_long, c_uint, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;
use std::{ptr, slice};

const APR_HASH_KEY_STRING: isize = -1;
const SVN_DIRENT_KIND: c_uint = 0x0000_0001;
type RawDirListing = (BTreeMap<String, NodeKind>, BTreeMap<String, String>);
const SVN_PROP_REVISION_AUTHOR: &[u8] = b"svn:author\0";
const SVN_PROP_REVISION_DATE: &[u8] = b"svn:date\0";
const SVN_PROP_REVISION_LOG: &[u8] = b"svn:log\0";

impl LibSvnBackend {
    pub(crate) fn node_property_bytes_at_repository_path(
        &self,
        repository_root: &str,
        path: &str,
        revision: u32,
    ) -> Result<BTreeMap<String, Vec<u8>>, String> {
        let rooted = Self {
            url: Some(repository_root.to_string()),
            repos_root: OnceLock::new(),
            config_dir: self.config_dir.clone(),
            username: self.username.clone(),
            password: self.password.clone(),
            no_auth_cache: self.no_auth_cache,
            auth_prompt: self.auth_prompt.clone(),
        };
        rooted.with_session(|session, pool| unsafe {
            let revision: c_long = revision
                .try_into()
                .map_err(|_| format!("SVN revision {revision} does not fit in svn_revnum_t"))?;
            directory_property_bytes(session, pool, path, revision)
        })
    }

    pub(super) fn with_session<T>(
        &self,
        operation: impl FnOnce(*mut SvnRaSessionT, *mut AprPoolT) -> Result<T, String>,
    ) -> Result<T, String> {
        let url = self
            .url
            .as_deref()
            .ok_or_else(|| "libsvn backend requires an SVN repository URL".to_string())?;
        let canonical_url = canonical_libsvn_url(url);
        let url = CString::new(canonical_url)
            .map_err(|_| "SVN repository URL contains NUL".to_string())?;
        AprRuntime::initialize()?;
        let apr = AprRuntime;
        let pool = apr.create_pool()?;

        unsafe {
            svn_call(svn_ra_initialize(pool.as_ptr()), "svn_ra_initialize")?;

            let mut callbacks: *mut SvnRaCallbacks2T = ptr::null_mut();
            svn_call(
                svn_ra_create_callbacks(&mut callbacks, pool.as_ptr()),
                "svn_ra_create_callbacks",
            )?;
            let auth_baton = self.auth_baton(pool.as_ptr())?;
            if !callbacks.is_null() {
                (*callbacks).auth_baton = auth_baton;
            }

            let config = self.config_hash(pool.as_ptr())?;

            let mut session: *mut SvnRaSessionT = ptr::null_mut();
            svn_call(
                svn_ra_open5(
                    &mut session,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    url.as_ptr(),
                    ptr::null(),
                    callbacks,
                    ptr::null_mut(),
                    config,
                    pool.as_ptr(),
                ),
                "svn_ra_open5",
            )?;

            if session.is_null() {
                return Err("libsvn returned a null RA session".to_string());
            }

            operation(session, pool.as_ptr())
        }
    }

    pub(super) fn read_repos_root(&self) -> Result<String, String> {
        self.with_session(|session, pool| unsafe {
            let mut repos_root = ptr::null();
            svn_call(
                svn_ra_get_repos_root2(session, &mut repos_root, pool),
                "svn_ra_get_repos_root2",
            )?;
            if repos_root.is_null() {
                return Err("libsvn returned a null repository root URL".to_string());
            }
            Ok(CStr::from_ptr(repos_root).to_string_lossy().into_owned())
        })
    }

    pub(super) fn session_repository_path(&self) -> String {
        let Some(url) = self.url.as_deref() else {
            return String::new();
        };
        let root = self.repos_root();
        url_path_in_repository(Some(root), url, "session URL").unwrap_or_default()
    }

    fn config_hash(&self, pool: *mut AprPoolT) -> Result<*mut AprHashT, String> {
        let Some(config_dir) = self.config_dir.as_deref() else {
            return Ok(ptr::null_mut());
        };
        let config_dir = CString::new(config_dir)
            .map_err(|_| "SVN config directory contains NUL".to_string())?;
        let mut config = ptr::null_mut();
        unsafe {
            svn_call(
                svn_config_get_config(&mut config, config_dir.as_ptr(), pool),
                "svn_config_get_config",
            )?;
        }
        Ok(config)
    }
}

pub(super) unsafe extern "C" fn receive_log_entry(
    baton: *mut c_void,
    log_entry: *mut svn_log_entry_t,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if baton.is_null() || log_entry.is_null() {
        return unsafe { callback_error_message("libsvn log callback had a null baton or entry") };
    }

    match catch_unwind(AssertUnwindSafe(|| {
        let revisions = unsafe { &mut *(baton as *mut Vec<RevisionEvent>) };
        let log_entry = unsafe { &*log_entry };
        if log_entry.revision < 0 {
            return Ok(());
        }
        let revision = u32::try_from(log_entry.revision).map_err(|_| {
            format!(
                "SVN log revision {} does not fit in u32",
                log_entry.revision
            )
        })?;
        revisions.push(RevisionEvent {
            revision,
            author: unsafe {
                revprop_string(log_entry.revprops, SVN_PROP_REVISION_AUTHOR.as_ptr())
            },
            message: unsafe { revprop_string(log_entry.revprops, SVN_PROP_REVISION_LOG.as_ptr()) },
            timestamp: unsafe {
                revprop_string(log_entry.revprops, SVN_PROP_REVISION_DATE.as_ptr())
            },
            changed_paths: unsafe { changed_paths(log_entry.changed_paths2) },
        });
        Ok::<(), String>(())
    })) {
        Ok(Ok(())) => ptr::null_mut(),
        Ok(Err(error)) => unsafe { callback_error_message(&error) },
        Err(_) => unsafe { callback_error_message("libsvn log callback panicked") },
    }
}

unsafe fn revprop_string(revprops: *mut AprHashT, name: *const u8) -> String {
    if revprops.is_null() {
        return String::new();
    }
    let value = unsafe { apr_hash_get(revprops, name.cast::<c_void>(), APR_HASH_KEY_STRING) }
        as *const svn_string_t;
    unsafe { svn_string_to_string(value) }
}

unsafe fn svn_string_to_string(value: *const svn_string_t) -> String {
    if value.is_null() {
        return String::new();
    }
    let value = unsafe { &*value };
    if value.data.is_null() || value.len == 0 {
        return String::new();
    }
    let bytes = unsafe { slice::from_raw_parts(value.data.cast::<u8>(), value.len) };
    String::from_utf8_lossy(bytes).into_owned()
}

unsafe fn svn_string_to_bytes(value: *const svn_string_t) -> Vec<u8> {
    if value.is_null() {
        return Vec::new();
    }
    let value = unsafe { &*value };
    if value.data.is_null() || value.len == 0 {
        return Vec::new();
    }
    unsafe { slice::from_raw_parts(value.data.cast::<u8>(), value.len) }.to_vec()
}

unsafe fn changed_paths(paths: *mut AprHashT) -> Vec<ChangedPath> {
    let mut changed_paths = Vec::new();
    if paths.is_null() {
        return changed_paths;
    }

    let mut index = unsafe { apr_hash_first(ptr::null_mut(), paths) };
    while !index.is_null() {
        let mut key: *const c_void = ptr::null();
        let mut key_len: isize = 0;
        let mut value: *mut c_void = ptr::null_mut();
        unsafe { apr_hash_this(index, &mut key, &mut key_len, &mut value) };
        if let Some(path) = unsafe { hash_key_to_string(key, key_len) } {
            let changed_path = value as *const svn_log_changed_path2_t;
            if let Some(change) = unsafe { changed_path_to_rust(path, changed_path) } {
                changed_paths.push(change);
            }
        }
        index = unsafe { apr_hash_next(index) };
    }

    changed_paths
}

unsafe fn hash_key_to_string(key: *const c_void, key_len: isize) -> Option<String> {
    if key.is_null() {
        return None;
    }
    if key_len >= 0 {
        let bytes = unsafe { slice::from_raw_parts(key.cast::<u8>(), key_len as usize) };
        Some(String::from_utf8_lossy(bytes).into_owned())
    } else if key_len == APR_HASH_KEY_STRING {
        Some(
            unsafe { CStr::from_ptr(key.cast::<c_char>()) }
                .to_string_lossy()
                .into_owned(),
        )
    } else {
        None
    }
}

unsafe fn changed_path_to_rust(
    path: String,
    changed_path: *const svn_log_changed_path2_t,
) -> Option<ChangedPath> {
    if changed_path.is_null() {
        return None;
    }
    let changed_path = unsafe { &*changed_path };
    Some(ChangedPath {
        path,
        action: match changed_path.action as u8 as char {
            'A' => ChangeAction::Add,
            'M' => ChangeAction::Modify,
            'D' => ChangeAction::Delete,
            'R' => ChangeAction::Replace,
            _ => return None,
        },
        copy_from_path: if changed_path.copyfrom_path.is_null() {
            None
        } else {
            Some(
                unsafe { CStr::from_ptr(changed_path.copyfrom_path) }
                    .to_string_lossy()
                    .into_owned(),
            )
        },
        copy_from_rev: u32::try_from(changed_path.copyfrom_rev).ok(),
        kind: node_kind_from_raw(changed_path.node_kind),
        properties_modified: changed_path.props_modified == 1,
        content_modified: changed_path.text_modified == 1,
        properties: BTreeMap::new(),
        content: None,
    })
}

fn node_kind_from_raw(kind: c_int) -> NodeKind {
    match kind {
        1 => NodeKind::File,
        2 => NodeKind::Directory,
        4 => NodeKind::Symlink,
        _ => NodeKind::Directory,
    }
}

pub(super) fn svn_node_kind_from_raw(kind: c_int) -> Option<SvnNodeKind> {
    match kind {
        0 => None,
        1 => Some(SvnNodeKind::File),
        2 => Some(SvnNodeKind::Directory),
        4 => Some(SvnNodeKind::Symlink),
        _ => None,
    }
}

pub(super) fn svn_node_kind_from_change_kind(kind: &NodeKind) -> SvnNodeKind {
    match kind {
        NodeKind::File => SvnNodeKind::File,
        NodeKind::Directory => SvnNodeKind::Directory,
        NodeKind::Symlink => SvnNodeKind::Symlink,
    }
}

unsafe fn fill_file_details(
    session: *mut SvnRaSessionT,
    pool: *mut AprPoolT,
    session_path: &str,
    revisions: &mut [RevisionEvent],
) -> Result<(), String> {
    for revision in revisions {
        let revision_number: c_long = revision.revision.try_into().map_err(|_| {
            format!(
                "SVN revision {} does not fit in svn_revnum_t",
                revision.revision
            )
        })?;
        let mut known_paths = revision
            .changed_paths
            .iter()
            .map(|path| path.path.clone())
            .collect::<BTreeSet<_>>();
        let mut copied_files = Vec::new();
        for path in &mut revision.changed_paths {
            if matches!(
                path.action,
                ChangeAction::Add | ChangeAction::Modify | ChangeAction::Replace
            ) && path.kind == NodeKind::File
            {
                let file_path = session_relative_path(session_path, &path.path);
                let (content, properties) =
                    unsafe { get_file(session, pool, &file_path, revision_number) }?;
                if path.action == ChangeAction::Modify && revision_number > 0 {
                    let (previous_content, previous_properties) =
                        unsafe { get_file(session, pool, &file_path, revision_number - 1) }?;
                    path.content_modified = content != previous_content;
                    path.properties_modified = properties != previous_properties;
                }
                path.content = Some(content);
                path.properties = properties;
            }
            if matches!(
                path.action,
                ChangeAction::Add | ChangeAction::Modify | ChangeAction::Replace
            ) && path.kind == NodeKind::Directory
            {
                let directory_path = session_relative_path(session_path, &path.path);
                let (_, properties) =
                    unsafe { dir_listing(session, pool, &directory_path, revision_number) }?;
                if path.action == ChangeAction::Modify && revision_number > 0 {
                    let (_, previous_properties) = unsafe {
                        dir_listing(session, pool, &directory_path, revision_number - 1)
                    }?;
                    path.properties_modified = properties != previous_properties;
                }
                path.properties = properties;
            }

            if !matches!(path.action, ChangeAction::Add | ChangeAction::Replace)
                || path.kind != NodeKind::Directory
                || path.copy_from_path.is_none()
            {
                continue;
            }
            let directory_path = session_relative_path(session_path, &path.path);
            for relative in unsafe { list_files(session, pool, &directory_path, revision_number) }?
            {
                let file_path = format!("{}/{}", path.path.trim_end_matches('/'), relative);
                if !known_paths.insert(file_path.clone()) {
                    continue;
                }
                let file_session_path = session_relative_path(session_path, &file_path);
                let (content, properties) =
                    unsafe { get_file(session, pool, &file_session_path, revision_number) }?;
                copied_files.push(ChangedPath {
                    path: file_path,
                    action: ChangeAction::Add,
                    copy_from_path: path
                        .copy_from_path
                        .as_ref()
                        .map(|source| format!("{}/{}", source.trim_end_matches('/'), relative)),
                    copy_from_rev: path.copy_from_rev,
                    kind: NodeKind::File,
                    properties_modified: true,
                    content_modified: true,
                    properties,
                    content: Some(content),
                });
            }
        }
        revision.changed_paths.extend(copied_files);
    }
    Ok(())
}

unsafe fn list_files(
    session: *mut SvnRaSessionT,
    pool: *mut AprPoolT,
    path: &str,
    revision: c_long,
) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    unsafe { list_files_recursive(session, pool, path, revision, "", &mut files) }?;
    Ok(files)
}

unsafe fn list_files_recursive(
    session: *mut SvnRaSessionT,
    pool: *mut AprPoolT,
    path: &str,
    revision: c_long,
    relative_prefix: &str,
    files: &mut Vec<String>,
) -> Result<(), String> {
    for (name, kind) in unsafe { dir_entries(session, pool, path, revision) }? {
        let child_relative = if relative_prefix.is_empty() {
            name.clone()
        } else {
            format!("{relative_prefix}/{name}")
        };
        let child_path = format!("{}/{}", path.trim_end_matches('/'), name);
        match kind {
            NodeKind::File | NodeKind::Symlink => files.push(child_relative),
            NodeKind::Directory => unsafe {
                list_files_recursive(session, pool, &child_path, revision, &child_relative, files)?
            },
        }
    }
    Ok(())
}

unsafe fn dir_entries(
    session: *mut SvnRaSessionT,
    pool: *mut AprPoolT,
    path: &str,
    revision: c_long,
) -> Result<BTreeMap<String, NodeKind>, String> {
    let relative_path = CString::new(path.trim_start_matches('/'))
        .map_err(|_| "SVN path contains NUL".to_string())?;
    let mut dirents = ptr::null_mut();
    unsafe {
        svn_call(
            svn_ra_get_dir2(
                session,
                &mut dirents,
                ptr::null_mut(),
                ptr::null_mut(),
                relative_path.as_ptr(),
                revision,
                SVN_DIRENT_KIND,
                pool,
            ),
            "svn_ra_get_dir2",
        )?;
    }

    let mut entries = BTreeMap::new();
    if dirents.is_null() {
        return Ok(entries);
    }

    let mut index = unsafe { apr_hash_first(ptr::null_mut(), dirents) };
    while !index.is_null() {
        let mut key: *const c_void = ptr::null();
        let mut key_len: isize = 0;
        let mut value: *mut c_void = ptr::null_mut();
        unsafe { apr_hash_this(index, &mut key, &mut key_len, &mut value) };
        if let Some(name) = unsafe { hash_key_to_string(key, key_len) } {
            let dirent = value as *const svn_dirent_t;
            if !dirent.is_null() {
                entries.insert(name, node_kind_from_raw(unsafe { (*dirent).kind }));
            }
        }
        index = unsafe { apr_hash_next(index) };
    }

    Ok(entries)
}

pub(super) unsafe fn dir_listing(
    session: *mut SvnRaSessionT,
    pool: *mut AprPoolT,
    path: &str,
    revision: c_long,
) -> Result<RawDirListing, String> {
    let relative_path = CString::new(path.trim_start_matches('/'))
        .map_err(|_| "SVN path contains NUL".to_string())?;
    let mut dirents = ptr::null_mut();
    let mut props = ptr::null_mut();
    unsafe {
        svn_call(
            svn_ra_get_dir2(
                session,
                &mut dirents,
                ptr::null_mut(),
                &mut props,
                relative_path.as_ptr(),
                revision,
                SVN_DIRENT_KIND,
                pool,
            ),
            "svn_ra_get_dir2",
        )?;
    }

    let mut entries = BTreeMap::new();
    if !dirents.is_null() {
        let mut index = unsafe { apr_hash_first(ptr::null_mut(), dirents) };
        while !index.is_null() {
            let mut key: *const c_void = ptr::null();
            let mut key_len: isize = 0;
            let mut value: *mut c_void = ptr::null_mut();
            unsafe { apr_hash_this(index, &mut key, &mut key_len, &mut value) };
            if let Some(name) = unsafe { hash_key_to_string(key, key_len) } {
                let dirent = value as *const svn_dirent_t;
                if !dirent.is_null() {
                    entries.insert(name, node_kind_from_raw(unsafe { (*dirent).kind }));
                }
            }
            index = unsafe { apr_hash_next(index) };
        }
    }

    Ok((entries, unsafe { svn_properties(props) }))
}

unsafe fn directory_property_bytes(
    session: *mut SvnRaSessionT,
    pool: *mut AprPoolT,
    path: &str,
    revision: c_long,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let relative_path =
        CString::new(path.trim_matches('/')).map_err(|_| "SVN path contains NUL".to_string())?;
    let mut props = ptr::null_mut();
    unsafe {
        svn_call(
            svn_ra_get_dir2(
                session,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut props,
                relative_path.as_ptr(),
                revision,
                0,
                pool,
            ),
            "svn_ra_get_dir2",
        )?;
        Ok(svn_property_bytes(props))
    }
}

pub(super) unsafe fn get_log(
    session: *mut SvnRaSessionT,
    pool: *mut AprPoolT,
    session_path: &str,
    paths: &[&str],
    start: u32,
    end: u32,
) -> Result<Vec<RevisionEvent>, String> {
    let path_strings = if paths.is_empty() {
        vec![CString::new("").expect("static path should not contain NUL")]
    } else {
        paths
            .iter()
            .map(|path| {
                CString::new(path.trim_matches('/'))
                    .map_err(|_| "SVN log path contains NUL".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    let pointer_size = std::mem::size_of::<*const c_char>()
        .try_into()
        .expect("pointer size should fit APR int");

    let log_paths = unsafe { apr_array_make(pool, path_strings.len() as c_int, pointer_size) };
    if log_paths.is_null() {
        return Err("APR returned a null paths array".to_string());
    }
    for path in &path_strings {
        unsafe {
            *(apr_array_push(log_paths) as *mut *const c_char) = path.as_ptr();
        }
    }

    let revprops = unsafe { apr_array_make(pool, 3, pointer_size) };
    if revprops.is_null() {
        return Err("APR returned a null revprops array".to_string());
    }
    unsafe {
        *(apr_array_push(revprops) as *mut *const c_char) =
            SVN_PROP_REVISION_AUTHOR.as_ptr().cast::<c_char>();
        *(apr_array_push(revprops) as *mut *const c_char) =
            SVN_PROP_REVISION_DATE.as_ptr().cast::<c_char>();
        *(apr_array_push(revprops) as *mut *const c_char) =
            SVN_PROP_REVISION_LOG.as_ptr().cast::<c_char>();
    }

    let start: c_long = start
        .try_into()
        .map_err(|_| format!("SVN revision {start} does not fit in svn_revnum_t"))?;
    let end: c_long = end
        .try_into()
        .map_err(|_| format!("SVN revision {end} does not fit in svn_revnum_t"))?;
    let mut revisions = Vec::<RevisionEvent>::new();
    unsafe {
        svn_call(
            svn_ra_get_log2(
                session,
                log_paths,
                start,
                end,
                0,
                1,
                0,
                0,
                revprops,
                receive_log_entry,
                (&mut revisions as *mut Vec<RevisionEvent>).cast::<c_void>(),
                pool,
            ),
            "svn_ra_get_log2",
        )?;
        if !session_path.trim_matches('/').is_empty() {
            for revision in &mut revisions {
                revision
                    .changed_paths
                    .retain(|path| repository_path_is_within(session_path, &path.path));
            }
        }
        fill_file_details(session, pool, session_path, &mut revisions)?;
    }
    if !session_path.trim_matches('/').is_empty() {
        for revision in &mut revisions {
            for path in &mut revision.changed_paths {
                path.path = session_editor_path(session_path, &path.path);
                if let Some(copy_from_path) = path.copy_from_path.as_mut()
                    && repository_path_is_within(session_path, copy_from_path)
                {
                    *copy_from_path = session_editor_path(session_path, copy_from_path);
                }
            }
        }
    }
    Ok(revisions)
}

pub(super) unsafe fn get_file(
    session: *mut SvnRaSessionT,
    pool: *mut AprPoolT,
    path: &str,
    revision: c_long,
) -> Result<(Vec<u8>, BTreeMap<String, String>), String> {
    let relative_path = CString::new(path.trim_start_matches('/'))
        .map_err(|_| "SVN path contains NUL".to_string())?;
    let buffer = unsafe { svn_stringbuf_create_empty(pool) };
    if buffer.is_null() {
        return Err("libsvn returned a null string buffer".to_string());
    }
    let stream = unsafe { svn_stream_from_stringbuf(buffer, pool) };
    if stream.is_null() {
        return Err("libsvn returned a null stream".to_string());
    }

    let mut props = ptr::null_mut();
    let result = unsafe {
        svn_call(
            svn_ra_get_file(
                session,
                relative_path.as_ptr(),
                revision,
                stream,
                ptr::null_mut(),
                &mut props,
                pool,
            ),
            "svn_ra_get_file",
        )
    };
    result?;

    let content = unsafe { stringbuf_bytes(buffer) };
    let properties = unsafe { svn_file_properties(props) };
    Ok((content, properties))
}

pub(super) unsafe fn stringbuf_bytes(buffer: *const svn_stringbuf_t) -> Vec<u8> {
    if buffer.is_null() {
        return Vec::new();
    }
    let buffer = unsafe { &*buffer };
    if buffer.data.is_null() || buffer.len == 0 {
        return Vec::new();
    }
    unsafe { slice::from_raw_parts(buffer.data.cast::<u8>(), buffer.len) }.to_vec()
}

unsafe fn svn_file_properties(props: *mut AprHashT) -> BTreeMap<String, String> {
    let mut properties = BTreeMap::new();
    if props.is_null() {
        return properties;
    }

    let mut index = unsafe { apr_hash_first(ptr::null_mut(), props) };
    while !index.is_null() {
        let mut key: *const c_void = ptr::null();
        let mut key_len: isize = 0;
        let mut value: *mut c_void = ptr::null_mut();
        unsafe { apr_hash_this(index, &mut key, &mut key_len, &mut value) };
        if let Some(name) = unsafe { hash_key_to_string(key, key_len) }
            && matches!(
                name.as_str(),
                "svn:executable"
                    | "svn:special"
                    | "svn:eol-style"
                    | "svn:mime-type"
                    | "svn:keywords"
                    | "svn:needs-lock"
            )
        {
            let value = unsafe { svn_string_to_string(value as *const svn_string_t) };
            if !value.is_empty() {
                properties.insert(name, value);
            }
        }
        index = unsafe { apr_hash_next(index) };
    }

    properties
}

unsafe fn svn_properties(props: *mut AprHashT) -> BTreeMap<String, String> {
    let mut properties = BTreeMap::new();
    if props.is_null() {
        return properties;
    }

    let mut index = unsafe { apr_hash_first(ptr::null_mut(), props) };
    while !index.is_null() {
        let mut key: *const c_void = ptr::null();
        let mut key_len: isize = 0;
        let mut value: *mut c_void = ptr::null_mut();
        unsafe { apr_hash_this(index, &mut key, &mut key_len, &mut value) };
        if let Some(name) = unsafe { hash_key_to_string(key, key_len) }
            && !name.starts_with("svn:entry:")
        {
            properties.insert(name, unsafe {
                svn_string_to_string(value as *const svn_string_t)
            });
        }
        index = unsafe { apr_hash_next(index) };
    }

    properties
}

pub(super) unsafe fn svn_property_bytes(props: *mut AprHashT) -> BTreeMap<String, Vec<u8>> {
    let mut properties = BTreeMap::new();
    if props.is_null() {
        return properties;
    }

    let mut index = unsafe { apr_hash_first(ptr::null_mut(), props) };
    while !index.is_null() {
        let mut key: *const c_void = ptr::null();
        let mut key_len: isize = 0;
        let mut value: *mut c_void = ptr::null_mut();
        unsafe { apr_hash_this(index, &mut key, &mut key_len, &mut value) };
        if let Some(name) = unsafe { hash_key_to_string(key, key_len) } {
            properties.insert(name, unsafe {
                svn_string_to_bytes(value as *const svn_string_t)
            });
        }
        index = unsafe { apr_hash_next(index) };
    }

    properties
}

pub(super) fn switch_url_path_in_repository(
    repository_url: Option<&str>,
    switch_url: &str,
) -> Result<String, String> {
    url_path_in_repository(repository_url, switch_url, "switch URL")
}

fn url_path_in_repository(
    repository_url: Option<&str>,
    url: &str,
    url_label: &str,
) -> Result<String, String> {
    let repository_url = repository_url
        .ok_or_else(|| "libsvn backend requires an SVN repository URL".to_string())?;
    let repository_url = canonical_libsvn_url(repository_url);
    let url = canonical_libsvn_url(url);
    let repository_url = repository_url.trim_end_matches('/');
    let url = url.trim_end_matches('/');
    let is_same_repository = url == repository_url
        || url
            .strip_prefix(repository_url)
            .is_some_and(|suffix| suffix.starts_with('/'));

    if url == repository_url {
        Ok(String::new())
    } else if is_same_repository {
        Ok(url[repository_url.len()..]
            .trim_start_matches('/')
            .to_string())
    } else {
        Err(format!(
            "{url_label} is outside repository root: {url} (root: {repository_url})"
        ))
    }
}

fn canonical_libsvn_url(url: &str) -> String {
    url.replace("%40", "@")
}

fn session_relative_path(session_path: &str, repository_path: &str) -> String {
    let session_path = session_path.trim_matches('/');
    let repository_path = repository_path.trim_matches('/');
    if session_path.is_empty() {
        return repository_path.to_string();
    }

    if repository_path == session_path {
        String::new()
    } else if let Some(path) = repository_path.strip_prefix(&format!("{session_path}/")) {
        path.to_string()
    } else {
        repository_path.to_string()
    }
}

fn repository_path_is_within(session_path: &str, repository_path: &str) -> bool {
    let session_path = session_path.trim_matches('/');
    let repository_path = repository_path.trim_matches('/');
    repository_path == session_path || repository_path.starts_with(&format!("{session_path}/"))
}

fn session_editor_path(session_path: &str, repository_path: &str) -> String {
    let relative = session_relative_path(session_path, repository_path);
    if relative.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", relative.trim_start_matches('/'))
    }
}
