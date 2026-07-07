use super::editor::FetchEditor;
#[cfg(git_svn_rs_libsvn_linked)]
use super::ra::DirEntry;
use super::ra::{DirListing, RaSession, SvnNodeKind};
#[cfg(git_svn_rs_libsvn_linked)]
use super::{ChangeAction, ChangedPath, NodeKind};
use super::{RevisionEvent, SvnBackend};
use crate::config::SvnRemoteConfig;
#[cfg(git_svn_rs_libsvn_linked)]
use std::collections::{BTreeMap, BTreeSet};
#[cfg(git_svn_rs_libsvn_linked)]
use std::ffi::{CStr, CString};
#[cfg(git_svn_rs_libsvn_linked)]
use std::os::raw::{c_char, c_int, c_long, c_uint, c_void};
#[cfg(git_svn_rs_libsvn_linked)]
use std::ptr;
#[cfg(git_svn_rs_libsvn_linked)]
use std::slice;
#[cfg(git_svn_rs_libsvn_linked)]
use std::sync::OnceLock;

pub const LIBSVN_NOT_LINKED_MESSAGE: &str =
    "libsvn backend is enabled but not linked: no libsvn FFI bindings are compiled into this build";
pub const LIBSVN_LINKED_PROBE_MESSAGE: &str =
    "libsvn link probe succeeded via vcpkg; backend API calls are not implemented yet";
pub const LIBSVN_NOT_IMPLEMENTED_MESSAGE: &str =
    "libsvn backend is linked, but native libsvn API calls are not implemented yet";
#[cfg(git_svn_rs_libsvn_linked)]
const APR_HASH_KEY_STRING: isize = -1;
#[cfg(git_svn_rs_libsvn_linked)]
const SVN_DIRENT_KIND: c_uint = 0x0000_0001;
#[cfg(git_svn_rs_libsvn_linked)]
const SVN_PROP_REVISION_AUTHOR: &[u8] = b"svn:author\0";
#[cfg(git_svn_rs_libsvn_linked)]
const SVN_PROP_REVISION_DATE: &[u8] = b"svn:date\0";
#[cfg(git_svn_rs_libsvn_linked)]
const SVN_PROP_REVISION_LOG: &[u8] = b"svn:log\0";
#[cfg(git_svn_rs_libsvn_linked)]
const SVN_AUTH_PARAM_DEFAULT_USERNAME: &[u8] = b"svn:auth:username\0";
#[cfg(git_svn_rs_libsvn_linked)]
const SVN_AUTH_PARAM_DEFAULT_PASSWORD: &[u8] = b"svn:auth:password\0";
#[cfg(git_svn_rs_libsvn_linked)]
const SVN_AUTH_PARAM_NO_AUTH_CACHE: &[u8] = b"svn:auth:no-auth-cache\0";
#[cfg(git_svn_rs_libsvn_linked)]
const SVN_AUTH_PRESENT_VALUE: &[u8] = b"1\0";

#[derive(Debug, Default)]
pub struct LibSvnBackend {
    #[cfg_attr(not(git_svn_rs_libsvn_linked), allow(dead_code))]
    url: Option<String>,
    #[cfg_attr(not(git_svn_rs_libsvn_linked), allow(dead_code))]
    config_dir: Option<String>,
    #[cfg_attr(not(git_svn_rs_libsvn_linked), allow(dead_code))]
    username: Option<String>,
    #[cfg_attr(not(git_svn_rs_libsvn_linked), allow(dead_code))]
    password: Option<String>,
    #[cfg_attr(not(git_svn_rs_libsvn_linked), allow(dead_code))]
    no_auth_cache: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibSvnLinkStatus {
    Linked,
    NotLinked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LibSvnAvailability {
    pub feature_enabled: bool,
    pub link_status: LibSvnLinkStatus,
    pub version: Option<&'static str>,
    pub detail: &'static str,
}

impl LibSvnBackend {
    pub fn new() -> Self {
        Self {
            url: None,
            config_dir: None,
            username: None,
            password: None,
            no_auth_cache: false,
        }
    }

    pub fn for_url(url: impl Into<String>) -> Self {
        Self {
            url: Some(url.into()),
            config_dir: None,
            username: None,
            password: None,
            no_auth_cache: false,
        }
    }

    pub fn from_config(config: &SvnRemoteConfig) -> Self {
        Self {
            url: Some(config.url.clone()),
            config_dir: config.config_dir.clone(),
            username: config.username.clone(),
            password: None,
            no_auth_cache: config.no_auth_cache,
        }
    }

    pub fn with_config_dir(mut self, config_dir: impl Into<String>) -> Self {
        self.config_dir = Some(config_dir.into());
        self
    }

    pub fn with_credentials(
        mut self,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        self.username = Some(username.into());
        self.password = Some(password.into());
        self
    }

    pub fn without_auth_cache(mut self) -> Self {
        self.no_auth_cache = true;
        self
    }

    pub fn availability() -> LibSvnAvailability {
        #[cfg(git_svn_rs_libsvn_linked)]
        {
            LibSvnAvailability {
                feature_enabled: true,
                link_status: LibSvnLinkStatus::Linked,
                version: None,
                detail: LIBSVN_LINKED_PROBE_MESSAGE,
            }
        }

        #[cfg(not(git_svn_rs_libsvn_linked))]
        {
            LibSvnAvailability {
                feature_enabled: true,
                link_status: LibSvnLinkStatus::NotLinked,
                version: None,
                detail: LIBSVN_NOT_LINKED_MESSAGE,
            }
        }
    }

    pub fn version(&self) -> Result<String, String> {
        #[cfg(git_svn_rs_libsvn_linked)]
        {
            let version = unsafe { svn_subr_version() };
            if version.is_null() {
                return Err("libsvn returned a null version record".to_string());
            }
            let version = unsafe { &*version };
            let tag = if version.tag.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(version.tag) }
                    .to_string_lossy()
                    .into_owned()
            };
            Ok(format!(
                "{}.{}.{}{}",
                version.major, version.minor, version.patch, tag
            ))
        }

        #[cfg(not(git_svn_rs_libsvn_linked))]
        Err(Self::unavailable_message().to_string())
    }

    #[cfg_attr(git_svn_rs_libsvn_linked, allow(dead_code))]
    fn unavailable_message() -> &'static str {
        if Self::availability().link_status == LibSvnLinkStatus::Linked {
            LIBSVN_NOT_IMPLEMENTED_MESSAGE
        } else {
            LIBSVN_NOT_LINKED_MESSAGE
        }
    }

    #[cfg(git_svn_rs_libsvn_linked)]
    fn with_session<T>(
        &self,
        operation: impl FnOnce(*mut SvnRaSessionT, *mut AprPoolT) -> Result<T, String>,
    ) -> Result<T, String> {
        let url = self
            .url
            .as_deref()
            .ok_or_else(|| "libsvn backend requires an SVN repository URL".to_string())?;
        let url = CString::new(url).map_err(|_| "SVN repository URL contains NUL".to_string())?;
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

    #[cfg(git_svn_rs_libsvn_linked)]
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

    #[cfg(git_svn_rs_libsvn_linked)]
    unsafe fn auth_baton(&self, pool: *mut AprPoolT) -> Result<*mut SvnAuthBatonT, String> {
        let provider_size = std::mem::size_of::<*mut SvnAuthProviderObjectT>()
            .try_into()
            .expect("pointer size should fit APR int");
        let providers = unsafe { apr_array_make(pool, 2, provider_size) };
        if providers.is_null() {
            return Err("APR returned a null auth provider array".to_string());
        }

        let mut simple_provider = ptr::null_mut();
        unsafe {
            svn_auth_get_simple_provider2(&mut simple_provider, None, ptr::null_mut(), pool);
        }
        if !simple_provider.is_null() {
            unsafe { push_auth_provider(providers, simple_provider) };
        }

        let mut username_provider = ptr::null_mut();
        unsafe { svn_auth_get_username_provider(&mut username_provider, pool) };
        if !username_provider.is_null() {
            unsafe { push_auth_provider(providers, username_provider) };
        }

        let mut auth_baton = ptr::null_mut();
        unsafe { svn_auth_open(&mut auth_baton, providers, pool) };
        if auth_baton.is_null() {
            return Err("libsvn returned a null auth baton".to_string());
        }

        if let Some(username) = &self.username {
            let username = pool_c_string(pool, username, "SVN username")?;
            unsafe {
                svn_auth_set_parameter(
                    auth_baton,
                    SVN_AUTH_PARAM_DEFAULT_USERNAME.as_ptr().cast::<c_char>(),
                    username.cast::<c_void>(),
                );
            }
        }
        if let Some(password) = &self.password {
            let password = pool_c_string(pool, password, "SVN password")?;
            unsafe {
                svn_auth_set_parameter(
                    auth_baton,
                    SVN_AUTH_PARAM_DEFAULT_PASSWORD.as_ptr().cast::<c_char>(),
                    password.cast::<c_void>(),
                );
            }
        }
        if self.no_auth_cache {
            unsafe {
                svn_auth_set_parameter(
                    auth_baton,
                    SVN_AUTH_PARAM_NO_AUTH_CACHE.as_ptr().cast::<c_char>(),
                    SVN_AUTH_PRESENT_VALUE.as_ptr().cast::<c_void>(),
                );
            }
        }

        Ok(auth_baton)
    }
}

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
struct svn_version_t {
    major: i32,
    minor: i32,
    patch: i32,
    tag: *const c_char,
}

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
struct svn_error_t {
    apr_err: c_int,
    message: *const c_char,
    child: *mut svn_error_t,
    pool: *mut AprPoolT,
    file: *const c_char,
    line: c_long,
}

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
struct svn_string_t {
    data: *const c_char,
    len: usize,
}

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
struct svn_stringbuf_t {
    pool: *mut AprPoolT,
    data: *mut c_char,
    len: usize,
    blocksize: usize,
}

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
struct svn_log_changed_path2_t {
    action: c_char,
    copyfrom_path: *const c_char,
    copyfrom_rev: c_long,
    node_kind: c_int,
    text_modified: c_int,
    props_modified: c_int,
}

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
struct svn_log_entry_t {
    changed_paths: *mut AprHashT,
    revision: c_long,
    revprops: *mut AprHashT,
    has_children: c_int,
    changed_paths2: *mut AprHashT,
    non_inheritable: c_int,
    subtractive_merge: c_int,
}

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
struct svn_dirent_t {
    kind: c_int,
    size: i64,
    has_props: c_int,
    created_rev: c_long,
    time: i64,
    last_author: *const c_char,
}

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
struct apr_array_header_t {
    pool: *mut AprPoolT,
    elt_size: c_int,
    nelts: c_int,
    nalloc: c_int,
    elts: *mut c_char,
}

#[cfg(git_svn_rs_libsvn_linked)]
enum AprPoolT {}

#[cfg(git_svn_rs_libsvn_linked)]
enum AprHashT {}

#[cfg(git_svn_rs_libsvn_linked)]
enum AprHashIndexT {}

#[cfg(git_svn_rs_libsvn_linked)]
enum SvnStreamT {}

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
struct SvnRaCallbacks2T {
    open_tmp_file: *mut c_void,
    auth_baton: *mut SvnAuthBatonT,
}

#[cfg(git_svn_rs_libsvn_linked)]
enum SvnAuthBatonT {}

#[cfg(git_svn_rs_libsvn_linked)]
enum SvnAuthProviderObjectT {}

#[cfg(git_svn_rs_libsvn_linked)]
enum SvnRaSessionT {}

#[cfg(git_svn_rs_libsvn_linked)]
struct AprRuntime;

#[cfg(git_svn_rs_libsvn_linked)]
impl AprRuntime {
    fn initialize() -> Result<(), String> {
        static APR_INIT: OnceLock<Result<(), String>> = OnceLock::new();
        APR_INIT
            .get_or_init(|| {
                let status = unsafe { apr_initialize() };
                if status == 0 {
                    Ok(())
                } else {
                    Err(format!("apr_initialize failed with status {status}"))
                }
            })
            .clone()
    }

    fn create_pool(&self) -> Result<AprPool, String> {
        let mut pool = ptr::null_mut();
        let status =
            unsafe { apr_pool_create_ex(&mut pool, ptr::null_mut(), None, ptr::null_mut()) };
        if status != 0 {
            return Err(format!("apr_pool_create_ex failed with status {status}"));
        }
        if pool.is_null() {
            return Err("APR returned a null pool".to_string());
        }
        Ok(AprPool { pool })
    }
}

#[cfg(git_svn_rs_libsvn_linked)]
struct AprPool {
    pool: *mut AprPoolT,
}

#[cfg(git_svn_rs_libsvn_linked)]
impl AprPool {
    fn as_ptr(&self) -> *mut AprPoolT {
        self.pool
    }
}

#[cfg(git_svn_rs_libsvn_linked)]
impl Drop for AprPool {
    fn drop(&mut self) {
        unsafe { apr_pool_destroy(self.pool) };
    }
}

#[cfg(git_svn_rs_libsvn_linked)]
type AprAbortFunc = Option<unsafe extern "C" fn(c_int) -> c_int>;
#[cfg(git_svn_rs_libsvn_linked)]
type SvnAuthPlaintextPromptFunc = Option<
    unsafe extern "C" fn(*mut c_int, *const c_char, *mut c_void, *mut AprPoolT) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn receive_log_entry(
    baton: *mut c_void,
    log_entry: *mut svn_log_entry_t,
    _pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if baton.is_null() || log_entry.is_null() {
        return ptr::null_mut();
    }

    let revisions = unsafe { &mut *(baton as *mut Vec<RevisionEvent>) };
    let log_entry = unsafe { &*log_entry };
    if log_entry.revision < 0 {
        return ptr::null_mut();
    }

    let Ok(revision) = u32::try_from(log_entry.revision) else {
        return ptr::null_mut();
    };

    revisions.push(RevisionEvent {
        revision,
        author: unsafe { revprop_string(log_entry.revprops, SVN_PROP_REVISION_AUTHOR.as_ptr()) },
        message: unsafe { revprop_string(log_entry.revprops, SVN_PROP_REVISION_LOG.as_ptr()) },
        timestamp: unsafe { revprop_string(log_entry.revprops, SVN_PROP_REVISION_DATE.as_ptr()) },
        changed_paths: unsafe { changed_paths(log_entry.changed_paths2) },
    });

    ptr::null_mut()
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe fn revprop_string(revprops: *mut AprHashT, name: *const u8) -> String {
    if revprops.is_null() {
        return String::new();
    }
    let value = unsafe { apr_hash_get(revprops, name.cast::<c_void>(), APR_HASH_KEY_STRING) }
        as *const svn_string_t;
    unsafe { svn_string_to_string(value) }
}

#[cfg(git_svn_rs_libsvn_linked)]
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

#[cfg(git_svn_rs_libsvn_linked)]
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

#[cfg(git_svn_rs_libsvn_linked)]
unsafe fn hash_key_to_string(key: *const c_void, key_len: isize) -> Option<String> {
    if key.is_null() {
        return None;
    }
    if key_len >= 0 {
        let bytes = unsafe { slice::from_raw_parts(key.cast::<u8>(), key_len as usize) };
        Some(String::from_utf8_lossy(bytes).into_owned())
    } else {
        Some(
            unsafe { CStr::from_ptr(key.cast::<c_char>()) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}

#[cfg(git_svn_rs_libsvn_linked)]
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
        properties: BTreeMap::new(),
        content: None,
    })
}

#[cfg(git_svn_rs_libsvn_linked)]
fn node_kind_from_raw(kind: c_int) -> NodeKind {
    match kind {
        1 => NodeKind::File,
        2 => NodeKind::Directory,
        4 => NodeKind::Symlink,
        _ => NodeKind::Directory,
    }
}

#[cfg(git_svn_rs_libsvn_linked)]
fn svn_node_kind_from_raw(kind: c_int) -> Option<SvnNodeKind> {
    match kind {
        0 => None,
        1 => Some(SvnNodeKind::File),
        2 => Some(SvnNodeKind::Directory),
        4 => Some(SvnNodeKind::Symlink),
        _ => None,
    }
}

#[cfg(git_svn_rs_libsvn_linked)]
fn svn_node_kind_from_change_kind(kind: &NodeKind) -> SvnNodeKind {
    match kind {
        NodeKind::File => SvnNodeKind::File,
        NodeKind::Directory => SvnNodeKind::Directory,
        NodeKind::Symlink => SvnNodeKind::Symlink,
    }
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe fn fill_file_details(
    session: *mut SvnRaSessionT,
    pool: *mut AprPoolT,
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
                let (content, properties) =
                    unsafe { get_file(session, pool, &path.path, revision_number) }?;
                path.content = Some(content);
                path.properties = properties;
            }

            if !matches!(path.action, ChangeAction::Add | ChangeAction::Replace)
                || path.kind != NodeKind::Directory
                || path.copy_from_path.is_none()
            {
                continue;
            }
            for relative in unsafe { list_files(session, pool, &path.path, revision_number) }? {
                let file_path = format!("{}/{}", path.path.trim_end_matches('/'), relative);
                if !known_paths.insert(file_path.clone()) {
                    continue;
                }
                let (content, properties) =
                    unsafe { get_file(session, pool, &file_path, revision_number) }?;
                copied_files.push(ChangedPath {
                    path: file_path,
                    action: ChangeAction::Add,
                    copy_from_path: path
                        .copy_from_path
                        .as_ref()
                        .map(|source| format!("{}/{}", source.trim_end_matches('/'), relative)),
                    copy_from_rev: path.copy_from_rev,
                    kind: NodeKind::File,
                    properties,
                    content: Some(content),
                });
            }
        }
        revision.changed_paths.extend(copied_files);
    }
    Ok(())
}

#[cfg(git_svn_rs_libsvn_linked)]
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

#[cfg(git_svn_rs_libsvn_linked)]
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

#[cfg(git_svn_rs_libsvn_linked)]
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

#[cfg(git_svn_rs_libsvn_linked)]
unsafe fn get_log(
    session: *mut SvnRaSessionT,
    pool: *mut AprPoolT,
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
    let mut revisions = Vec::new();
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
        fill_file_details(session, pool, &mut revisions)?;
    }
    Ok(revisions)
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe fn push_auth_provider(
    array: *mut apr_array_header_t,
    provider: *mut SvnAuthProviderObjectT,
) {
    unsafe {
        *(apr_array_push(array) as *mut *mut SvnAuthProviderObjectT) = provider;
    }
}

#[cfg(git_svn_rs_libsvn_linked)]
fn pool_c_string(pool: *mut AprPoolT, value: &str, label: &str) -> Result<*const c_char, String> {
    let value = CString::new(value).map_err(|_| format!("{label} contains NUL"))?;
    let value = unsafe { apr_pstrdup(pool, value.as_ptr()) };
    if value.is_null() {
        Err(format!("APR failed to allocate {label}"))
    } else {
        Ok(value)
    }
}

#[cfg(git_svn_rs_libsvn_linked)]
fn replay_revision(
    revision: &RevisionEvent,
    path: &str,
    editor: &mut dyn FetchEditor,
) -> Result<(), String> {
    let normalized_path = normalize_ra_path(path);
    editor.open_root(revision.revision)?;
    for changed_path in &revision.changed_paths {
        if !path_matches(&changed_path.path, &normalized_path) {
            continue;
        }
        let editor_path = editor_path(&changed_path.path);
        match changed_path.action {
            ChangeAction::Delete => {
                editor.delete_entry(&editor_path, revision.revision.saturating_sub(1))?;
            }
            ChangeAction::Add | ChangeAction::Replace => match changed_path.kind {
                NodeKind::Directory => {
                    let copy_from =
                        editor_copy_from(&changed_path.copy_from_path, changed_path.copy_from_rev);
                    editor.add_directory(
                        &editor_path,
                        copy_from
                            .as_ref()
                            .map(|(path, revision)| (path.as_str(), *revision)),
                    )?
                }
                NodeKind::File | NodeKind::Symlink => {
                    let copy_from =
                        editor_copy_from(&changed_path.copy_from_path, changed_path.copy_from_rev);
                    editor.add_file(
                        &editor_path,
                        copy_from
                            .as_ref()
                            .map(|(path, revision)| (path.as_str(), *revision)),
                    )?;
                    replay_file_details(&editor_path, changed_path, editor)?;
                }
            },
            ChangeAction::Modify => {
                if matches!(changed_path.kind, NodeKind::File | NodeKind::Symlink) {
                    replay_file_details(&editor_path, changed_path, editor)?;
                }
            }
        }
    }
    editor.close_edit()
}

#[cfg(git_svn_rs_libsvn_linked)]
fn replay_file_details(
    editor_path: &str,
    changed_path: &ChangedPath,
    editor: &mut dyn FetchEditor,
) -> Result<(), String> {
    for name in ["svn:executable", "svn:special"] {
        editor.change_file_prop(
            editor_path,
            name,
            changed_path.properties.get(name).map(String::as_str),
        )?;
    }
    for (name, value) in &changed_path.properties {
        if !matches!(name.as_str(), "svn:executable" | "svn:special") {
            editor.change_file_prop(editor_path, name, Some(value))?;
        }
    }
    if let Some(content) = &changed_path.content {
        editor.apply_textdelta(editor_path, content)?;
    }
    Ok(())
}

#[cfg(git_svn_rs_libsvn_linked)]
fn editor_copy_from(path: &Option<String>, revision: Option<u32>) -> Option<(String, u32)> {
    path.as_ref()
        .zip(revision)
        .map(|(path, revision)| (editor_path(path), revision))
}

#[cfg(git_svn_rs_libsvn_linked)]
fn normalize_ra_path(path: &str) -> String {
    path.trim_matches('/').to_string()
}

#[cfg(git_svn_rs_libsvn_linked)]
fn editor_path(path: &str) -> String {
    path.trim_start_matches('/').to_string()
}

#[cfg(git_svn_rs_libsvn_linked)]
fn path_matches(changed_path: &str, path: &str) -> bool {
    let changed_path = changed_path.trim_matches('/');
    path.is_empty() || changed_path == path || changed_path.starts_with(&format!("{path}/"))
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe fn get_file(
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

#[cfg(git_svn_rs_libsvn_linked)]
unsafe fn stringbuf_bytes(buffer: *const svn_stringbuf_t) -> Vec<u8> {
    if buffer.is_null() {
        return Vec::new();
    }
    let buffer = unsafe { &*buffer };
    if buffer.data.is_null() || buffer.len == 0 {
        return Vec::new();
    }
    unsafe { slice::from_raw_parts(buffer.data.cast::<u8>(), buffer.len) }.to_vec()
}

#[cfg(git_svn_rs_libsvn_linked)]
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
            && matches!(name.as_str(), "svn:executable" | "svn:special")
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

#[cfg(git_svn_rs_libsvn_linked)]
unsafe fn svn_call(error: *mut svn_error_t, context: &str) -> Result<(), String> {
    if error.is_null() {
        Ok(())
    } else {
        let mut buffer = [0i8; 512];
        let message = unsafe { svn_err_best_message(error, buffer.as_mut_ptr(), buffer.len()) };
        let detail = if message.is_null() {
            context.to_string()
        } else {
            unsafe { CStr::from_ptr(message) }
                .to_string_lossy()
                .into_owned()
        };
        unsafe { svn_error_clear(error) };
        Err(format!("{context} failed: {detail}"))
    }
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" {
    fn apr_initialize() -> c_int;
    fn apr_pool_create_ex(
        newpool: *mut *mut AprPoolT,
        parent: *mut AprPoolT,
        abort_fn: AprAbortFunc,
        allocator: *mut c_void,
    ) -> c_int;
    fn apr_pool_destroy(pool: *mut AprPoolT);
    fn apr_pstrdup(pool: *mut AprPoolT, string: *const c_char) -> *const c_char;
    fn apr_array_make(
        pool: *mut AprPoolT,
        nelts: c_int,
        elt_size: c_int,
    ) -> *mut apr_array_header_t;
    fn apr_array_push(array: *mut apr_array_header_t) -> *mut c_void;
    fn apr_hash_get(hash: *mut AprHashT, key: *const c_void, key_len: isize) -> *mut c_void;
    fn apr_hash_first(pool: *mut AprPoolT, hash: *mut AprHashT) -> *mut AprHashIndexT;
    fn apr_hash_next(index: *mut AprHashIndexT) -> *mut AprHashIndexT;
    fn apr_hash_this(
        index: *mut AprHashIndexT,
        key: *mut *const c_void,
        key_len: *mut isize,
        value: *mut *mut c_void,
    );
    fn svn_stringbuf_create_empty(pool: *mut AprPoolT) -> *mut svn_stringbuf_t;
    fn svn_stream_from_stringbuf(
        buffer: *mut svn_stringbuf_t,
        pool: *mut AprPoolT,
    ) -> *mut SvnStreamT;
    fn svn_subr_version() -> *const svn_version_t;
    fn svn_err_best_message(
        error: *const svn_error_t,
        buffer: *mut c_char,
        buffer_size: usize,
    ) -> *const c_char;
    fn svn_error_clear(error: *mut svn_error_t);
    fn svn_auth_get_simple_provider2(
        provider: *mut *mut SvnAuthProviderObjectT,
        plaintext_prompt_func: SvnAuthPlaintextPromptFunc,
        prompt_baton: *mut c_void,
        pool: *mut AprPoolT,
    );
    fn svn_auth_get_username_provider(
        provider: *mut *mut SvnAuthProviderObjectT,
        pool: *mut AprPoolT,
    );
    fn svn_auth_open(
        auth_baton: *mut *mut SvnAuthBatonT,
        providers: *const apr_array_header_t,
        pool: *mut AprPoolT,
    );
    fn svn_auth_set_parameter(
        auth_baton: *mut SvnAuthBatonT,
        name: *const c_char,
        value: *const c_void,
    );
    fn svn_config_get_config(
        config: *mut *mut AprHashT,
        config_dir: *const c_char,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    fn svn_ra_initialize(pool: *mut AprPoolT) -> *mut svn_error_t;
    fn svn_ra_create_callbacks(
        callbacks: *mut *mut SvnRaCallbacks2T,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    fn svn_ra_open5(
        session: *mut *mut SvnRaSessionT,
        corrected_url: *mut *const c_char,
        redirect_url: *mut *const c_char,
        repos_url: *const c_char,
        uuid: *const c_char,
        callbacks: *const SvnRaCallbacks2T,
        callback_baton: *mut c_void,
        config: *mut AprHashT,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    fn svn_ra_get_latest_revnum(
        session: *mut SvnRaSessionT,
        latest_revnum: *mut c_long,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    fn svn_ra_check_path(
        session: *mut SvnRaSessionT,
        path: *const c_char,
        revision: c_long,
        kind: *mut c_int,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    fn svn_ra_get_uuid2(
        session: *mut SvnRaSessionT,
        uuid: *mut *const c_char,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    fn svn_ra_get_file(
        session: *mut SvnRaSessionT,
        path: *const c_char,
        revision: c_long,
        stream: *mut SvnStreamT,
        fetched_rev: *mut c_long,
        props: *mut *mut AprHashT,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    fn svn_ra_get_dir2(
        session: *mut SvnRaSessionT,
        dirents: *mut *mut AprHashT,
        fetched_rev: *mut c_long,
        props: *mut *mut AprHashT,
        path: *const c_char,
        revision: c_long,
        dirent_fields: c_uint,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    fn svn_ra_get_log2(
        session: *mut SvnRaSessionT,
        paths: *const apr_array_header_t,
        start: c_long,
        end: c_long,
        limit: c_int,
        discover_changed_paths: c_int,
        strict_node_history: c_int,
        include_merged_revisions: c_int,
        revprops: *const apr_array_header_t,
        receiver: unsafe extern "C" fn(
            *mut c_void,
            *mut svn_log_entry_t,
            *mut AprPoolT,
        ) -> *mut svn_error_t,
        receiver_baton: *mut c_void,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
}

impl SvnBackend for LibSvnBackend {
    fn uuid(&self) -> Result<String, String> {
        #[cfg(git_svn_rs_libsvn_linked)]
        {
            self.with_session(|session, pool| unsafe {
                let mut uuid = ptr::null();
                svn_call(
                    svn_ra_get_uuid2(session, &mut uuid, pool),
                    "svn_ra_get_uuid2",
                )?;
                if uuid.is_null() {
                    return Err("libsvn returned a null repository UUID".to_string());
                }
                Ok(CStr::from_ptr(uuid).to_string_lossy().into_owned())
            })
        }

        #[cfg(not(git_svn_rs_libsvn_linked))]
        Err(Self::unavailable_message().to_string())
    }

    fn latest_revnum(&self) -> Result<u32, String> {
        #[cfg(git_svn_rs_libsvn_linked)]
        {
            self.with_session(|session, pool| unsafe {
                let mut latest_revnum = 0;
                svn_call(
                    svn_ra_get_latest_revnum(session, &mut latest_revnum, pool),
                    "svn_ra_get_latest_revnum",
                )?;
                u32::try_from(latest_revnum)
                    .map_err(|_| format!("SVN revision {latest_revnum} does not fit in u32"))
            })
        }

        #[cfg(not(git_svn_rs_libsvn_linked))]
        Err(Self::unavailable_message().to_string())
    }

    fn log(&self, start: u32, end: u32) -> Result<Vec<RevisionEvent>, String> {
        #[cfg(git_svn_rs_libsvn_linked)]
        {
            self.with_session(|session, pool| unsafe { get_log(session, pool, &[], start, end) })
        }

        #[cfg(not(git_svn_rs_libsvn_linked))]
        {
            let _ = (start, end);
            Err(Self::unavailable_message().to_string())
        }
    }
}

impl RaSession for LibSvnBackend {
    fn url(&self) -> &str {
        self.url.as_deref().unwrap_or_default()
    }

    fn repos_root(&self) -> &str {
        self.url()
    }

    fn uuid(&self) -> Result<String, String> {
        SvnBackend::uuid(self)
    }

    fn latest_revnum(&self) -> Result<u32, String> {
        SvnBackend::latest_revnum(self)
    }

    fn check_path(&self, path: &str, revision: u32) -> Result<Option<SvnNodeKind>, String> {
        #[cfg(git_svn_rs_libsvn_linked)]
        {
            self.with_session(|session, pool| unsafe {
                let path = CString::new(path.trim_matches('/'))
                    .map_err(|_| "SVN path contains NUL".to_string())?;
                let revision: c_long = revision
                    .try_into()
                    .map_err(|_| format!("SVN revision {revision} does not fit in svn_revnum_t"))?;
                let mut kind = 0;
                svn_call(
                    svn_ra_check_path(session, path.as_ptr(), revision, &mut kind, pool),
                    "svn_ra_check_path",
                )?;
                Ok(svn_node_kind_from_raw(kind))
            })
        }

        #[cfg(not(git_svn_rs_libsvn_linked))]
        {
            let _ = (path, revision);
            Err(Self::unavailable_message().to_string())
        }
    }

    fn get_dir(&self, path: &str, revision: u32) -> Result<DirListing, String> {
        #[cfg(git_svn_rs_libsvn_linked)]
        {
            self.with_session(|session, pool| unsafe {
                let revision_number: c_long = revision
                    .try_into()
                    .map_err(|_| format!("SVN revision {revision} does not fit in svn_revnum_t"))?;
                if self.check_path(path, revision)? != Some(SvnNodeKind::Directory) {
                    return Err(format!(
                        "path is not a directory at r{revision}: {}",
                        path.trim_matches('/')
                    ));
                }
                let entries = dir_entries(session, pool, path, revision_number)?
                    .into_iter()
                    .map(|(name, kind)| {
                        (
                            name,
                            DirEntry {
                                kind: svn_node_kind_from_change_kind(&kind),
                            },
                        )
                    })
                    .collect();
                Ok(DirListing {
                    entries,
                    properties: BTreeMap::new(),
                })
            })
        }

        #[cfg(not(git_svn_rs_libsvn_linked))]
        {
            let _ = (path, revision);
            Err(Self::unavailable_message().to_string())
        }
    }

    fn get_log(&self, paths: &[&str], start: u32, end: u32) -> Result<Vec<RevisionEvent>, String> {
        #[cfg(git_svn_rs_libsvn_linked)]
        {
            self.with_session(|session, pool| unsafe { get_log(session, pool, paths, start, end) })
        }

        #[cfg(not(git_svn_rs_libsvn_linked))]
        {
            let _ = (paths, start, end);
            Err(Self::unavailable_message().to_string())
        }
    }

    fn do_update(
        &self,
        path: &str,
        revision: u32,
        editor: &mut dyn FetchEditor,
    ) -> Result<(), String> {
        #[cfg(git_svn_rs_libsvn_linked)]
        {
            self.with_session(|session, pool| unsafe {
                let revisions = get_log(session, pool, &[path], revision, revision)?;
                let revision_event = revisions
                    .iter()
                    .find(|event| event.revision == revision)
                    .ok_or_else(|| format!("SVN revision r{revision} was not found"))?;
                replay_revision(revision_event, path, editor)
            })
        }

        #[cfg(not(git_svn_rs_libsvn_linked))]
        {
            let _ = (path, revision, editor);
            Err(Self::unavailable_message().to_string())
        }
    }

    fn do_switch(
        &self,
        path: &str,
        revision: u32,
        switch_url: &str,
        editor: &mut dyn FetchEditor,
    ) -> Result<(), String> {
        let _ = switch_url;
        self.do_update(path, revision, editor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
