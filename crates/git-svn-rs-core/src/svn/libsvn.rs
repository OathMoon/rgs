use super::auth::AuthPrompt;
#[cfg(git_svn_rs_libsvn_linked)]
use super::auth::AuthRequest;
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
use std::sync::Arc;
#[cfg(git_svn_rs_libsvn_linked)]
use std::sync::OnceLock;

pub const LIBSVN_NOT_LINKED_MESSAGE: &str =
    "libsvn backend is enabled but not linked: no libsvn FFI bindings are compiled into this build";
pub const LIBSVN_LINKED_PROBE_MESSAGE: &str =
    "libsvn link probe succeeded via vcpkg; native backend API calls are available";
pub const LIBSVN_NOT_IMPLEMENTED_MESSAGE: &str =
    "libsvn backend is linked, but native libsvn API calls are not implemented yet";
#[cfg(git_svn_rs_libsvn_linked)]
const APR_HASH_KEY_STRING: isize = -1;
#[cfg(git_svn_rs_libsvn_linked)]
const SVN_DIRENT_KIND: c_uint = 0x0000_0001;
#[cfg(git_svn_rs_libsvn_linked)]
type RawDirListing = (BTreeMap<String, NodeKind>, BTreeMap<String, String>);
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
#[cfg(git_svn_rs_libsvn_linked)]
#[allow(dead_code)]
const SVN_DEPTH_INFINITY: c_int = 3;

#[derive(Default)]
pub struct LibSvnBackend {
    #[cfg_attr(not(git_svn_rs_libsvn_linked), allow(dead_code))]
    url: Option<String>,
    #[cfg(git_svn_rs_libsvn_linked)]
    repos_root: OnceLock<String>,
    #[cfg_attr(not(git_svn_rs_libsvn_linked), allow(dead_code))]
    config_dir: Option<String>,
    #[cfg_attr(not(git_svn_rs_libsvn_linked), allow(dead_code))]
    username: Option<String>,
    #[cfg_attr(not(git_svn_rs_libsvn_linked), allow(dead_code))]
    password: Option<String>,
    #[cfg_attr(not(git_svn_rs_libsvn_linked), allow(dead_code))]
    no_auth_cache: bool,
    #[cfg_attr(not(git_svn_rs_libsvn_linked), allow(dead_code))]
    auth_prompt: Option<Arc<dyn AuthPrompt>>,
}

impl std::fmt::Debug for LibSvnBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibSvnBackend")
            .field("url", &self.url)
            .field("config_dir", &self.config_dir)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("no_auth_cache", &self.no_auth_cache)
            .field(
                "auth_prompt",
                &self.auth_prompt.as_ref().map(|_| "<present>"),
            )
            .finish()
    }
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
            #[cfg(git_svn_rs_libsvn_linked)]
            repos_root: OnceLock::new(),
            config_dir: None,
            username: None,
            password: None,
            no_auth_cache: false,
            auth_prompt: None,
        }
    }

    pub fn for_url(url: impl Into<String>) -> Self {
        Self {
            url: Some(url.into()),
            #[cfg(git_svn_rs_libsvn_linked)]
            repos_root: OnceLock::new(),
            config_dir: None,
            username: None,
            password: None,
            no_auth_cache: false,
            auth_prompt: None,
        }
    }

    pub fn from_config(config: &SvnRemoteConfig) -> Self {
        Self {
            url: Some(config.url.clone()),
            #[cfg(git_svn_rs_libsvn_linked)]
            repos_root: OnceLock::new(),
            config_dir: config.config_dir.clone(),
            username: config.username.clone(),
            password: None,
            no_auth_cache: config.no_auth_cache,
            auth_prompt: None,
        }
    }

    pub fn with_config_dir(mut self, config_dir: impl Into<String>) -> Self {
        self.config_dir = Some(config_dir.into());
        self
    }

    pub fn with_username(mut self, username: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self
    }

    pub fn with_password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
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

    #[cfg(test)]
    pub(crate) fn configured_username(&self) -> Option<&str> {
        self.username.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn configured_config_dir(&self) -> Option<&str> {
        self.config_dir.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn configured_password(&self) -> Option<&str> {
        self.password.as_deref()
    }

    pub fn without_auth_cache(mut self) -> Self {
        self.no_auth_cache = true;
        self
    }

    pub fn with_auth_prompt(mut self, prompt: impl AuthPrompt + 'static) -> Self {
        self.auth_prompt = Some(Arc::new(prompt));
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
    fn replay_log_update(
        &self,
        source_path: &str,
        target_path: &str,
        revision: u32,
        editor: &mut dyn FetchEditor,
    ) -> Result<(), String> {
        let session_path = self.session_repository_path();
        let replay_source_path = join_ra_paths(&session_path, source_path);
        self.with_session(|session, pool| unsafe {
            let revisions = get_log(
                session,
                pool,
                &session_path,
                &[source_path],
                revision,
                revision,
            )?;
            let revision_event = revisions
                .iter()
                .find(|event| event.revision == revision)
                .ok_or_else(|| format!("SVN revision r{revision} was not found"))?;
            replay_revision(revision_event, &replay_source_path, target_path, editor)
        })
    }

    #[cfg(git_svn_rs_libsvn_linked)]
    fn read_repos_root(&self) -> Result<String, String> {
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

    #[cfg(git_svn_rs_libsvn_linked)]
    fn session_repository_path(&self) -> String {
        let Some(url) = self.url.as_deref() else {
            return String::new();
        };
        let root = self.repos_root();
        url_path_in_repository(Some(root), url, "session URL").unwrap_or_default()
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
        let providers = unsafe { apr_array_make(pool, 3, provider_size) };
        if providers.is_null() {
            return Err("APR returned a null auth provider array".to_string());
        }

        if let Some(prompt) = &self.auth_prompt {
            let prompt_baton = unsafe { apr_palloc(pool, std::mem::size_of::<SimplePromptBaton>()) }
                as *mut SimplePromptBaton;
            if prompt_baton.is_null() {
                return Err("APR returned a null auth prompt baton".to_string());
            }
            unsafe {
                (*prompt_baton).prompt = prompt as *const Arc<dyn AuthPrompt>;
                (*prompt_baton).no_auth_cache = self.no_auth_cache;
            }
            let mut prompt_provider = ptr::null_mut();
            unsafe {
                svn_auth_get_simple_prompt_provider(
                    &mut prompt_provider,
                    Some(prompt_simple_credentials),
                    prompt_baton.cast::<c_void>(),
                    3,
                    pool,
                );
            }
            if !prompt_provider.is_null() {
                unsafe { push_auth_provider(providers, prompt_provider) };
            }
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
struct svn_auth_cred_simple_t {
    username: *const c_char,
    password: *const c_char,
    may_save: c_int,
}

#[cfg(git_svn_rs_libsvn_linked)]
type SvnDeltaSetTargetRevisionFunc = Option<
    unsafe extern "C" fn(
        edit_baton: *mut c_void,
        target_revision: c_long,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnDeltaCloseEditFunc =
    Option<unsafe extern "C" fn(edit_baton: *mut c_void, pool: *mut AprPoolT) -> *mut svn_error_t>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnDeltaOpenRootFunc = Option<
    unsafe extern "C" fn(
        edit_baton: *mut c_void,
        base_revision: c_long,
        dir_pool: *mut AprPoolT,
        root_baton: *mut *mut c_void,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnDeltaDeleteEntryFunc = Option<
    unsafe extern "C" fn(
        path: *const c_char,
        revision: c_long,
        parent_baton: *mut c_void,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnDeltaCloseDirectoryFunc =
    Option<unsafe extern "C" fn(dir_baton: *mut c_void, pool: *mut AprPoolT) -> *mut svn_error_t>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnDeltaAbsentDirectoryFunc = Option<
    unsafe extern "C" fn(
        path: *const c_char,
        parent_baton: *mut c_void,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnDeltaAddDirectoryFunc = Option<
    unsafe extern "C" fn(
        path: *const c_char,
        parent_baton: *mut c_void,
        copyfrom_path: *const c_char,
        copyfrom_revision: c_long,
        dir_pool: *mut AprPoolT,
        child_baton: *mut *mut c_void,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnDeltaOpenDirectoryFunc = Option<
    unsafe extern "C" fn(
        path: *const c_char,
        parent_baton: *mut c_void,
        base_revision: c_long,
        dir_pool: *mut AprPoolT,
        child_baton: *mut *mut c_void,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnDeltaAddFileFunc = Option<
    unsafe extern "C" fn(
        path: *const c_char,
        parent_baton: *mut c_void,
        copyfrom_path: *const c_char,
        copyfrom_revision: c_long,
        file_pool: *mut AprPoolT,
        file_baton: *mut *mut c_void,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnDeltaOpenFileFunc = Option<
    unsafe extern "C" fn(
        path: *const c_char,
        parent_baton: *mut c_void,
        base_revision: c_long,
        file_pool: *mut AprPoolT,
        file_baton: *mut *mut c_void,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnDeltaAbsentFileFunc = Option<
    unsafe extern "C" fn(
        path: *const c_char,
        parent_baton: *mut c_void,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnDeltaCloseFileFunc = Option<
    unsafe extern "C" fn(
        file_baton: *mut c_void,
        text_checksum: *const c_char,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnTxdeltaWindowHandlerFunc = Option<
    unsafe extern "C" fn(window: *mut SvnTxdeltaWindowT, baton: *mut c_void) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnDeltaApplyTextdeltaFunc = Option<
    unsafe extern "C" fn(
        file_baton: *mut c_void,
        base_checksum: *const c_char,
        result_pool: *mut AprPoolT,
        handler: *mut SvnTxdeltaWindowHandlerFunc,
        handler_baton: *mut *mut c_void,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnTxdeltaStreamOpenFunc = Option<
    unsafe extern "C" fn(
        txdelta_stream: *mut *mut SvnTxdeltaStreamT,
        baton: *mut c_void,
        result_pool: *mut AprPoolT,
        scratch_pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnDeltaApplyTextdeltaStreamFunc = Option<
    unsafe extern "C" fn(
        editor: *const SvnDeltaEditorT,
        file_baton: *mut c_void,
        base_checksum: *const c_char,
        open_func: SvnTxdeltaStreamOpenFunc,
        open_baton: *mut c_void,
        scratch_pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnDeltaChangeDirPropFunc = Option<
    unsafe extern "C" fn(
        dir_baton: *mut c_void,
        name: *const c_char,
        value: *const svn_string_t,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnDeltaChangeFilePropFunc = Option<
    unsafe extern "C" fn(
        file_baton: *mut c_void,
        name: *const c_char,
        value: *const svn_string_t,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnDeltaAbortEditFunc =
    Option<unsafe extern "C" fn(edit_baton: *mut c_void, pool: *mut AprPoolT) -> *mut svn_error_t>;

#[cfg(git_svn_rs_libsvn_linked)]
#[allow(dead_code)]
#[repr(C)]
struct SvnDeltaEditorT {
    set_target_revision: SvnDeltaSetTargetRevisionFunc,
    open_root: SvnDeltaOpenRootFunc,
    delete_entry: SvnDeltaDeleteEntryFunc,
    add_directory: SvnDeltaAddDirectoryFunc,
    open_directory: SvnDeltaOpenDirectoryFunc,
    change_dir_prop: SvnDeltaChangeDirPropFunc,
    close_directory: SvnDeltaCloseDirectoryFunc,
    absent_directory: SvnDeltaAbsentDirectoryFunc,
    add_file: SvnDeltaAddFileFunc,
    open_file: SvnDeltaOpenFileFunc,
    apply_textdelta: SvnDeltaApplyTextdeltaFunc,
    change_file_prop: SvnDeltaChangeFilePropFunc,
    close_file: SvnDeltaCloseFileFunc,
    absent_file: SvnDeltaAbsentFileFunc,
    close_edit: SvnDeltaCloseEditFunc,
    abort_edit: SvnDeltaAbortEditFunc,
    apply_textdelta_stream: SvnDeltaApplyTextdeltaStreamFunc,
}

#[cfg(git_svn_rs_libsvn_linked)]
type SvnRaReporterSetPathFunc = Option<
    unsafe extern "C" fn(
        report_baton: *mut c_void,
        path: *const c_char,
        revision: c_long,
        depth: c_int,
        start_empty: c_int,
        lock_token: *const c_char,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnRaReporterDeletePathFunc = Option<
    unsafe extern "C" fn(
        report_baton: *mut c_void,
        path: *const c_char,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnRaReporterLinkPathFunc = Option<
    unsafe extern "C" fn(
        report_baton: *mut c_void,
        path: *const c_char,
        url: *const c_char,
        revision: c_long,
        depth: c_int,
        start_empty: c_int,
        lock_token: *const c_char,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnRaReporterFinishReportFunc = Option<
    unsafe extern "C" fn(report_baton: *mut c_void, pool: *mut AprPoolT) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
type SvnRaReporterAbortReportFunc = Option<
    unsafe extern "C" fn(report_baton: *mut c_void, pool: *mut AprPoolT) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
#[allow(dead_code)]
#[repr(C)]
struct SvnRaReporter3T {
    set_path: SvnRaReporterSetPathFunc,
    delete_path: SvnRaReporterDeletePathFunc,
    link_path: SvnRaReporterLinkPathFunc,
    finish_report: SvnRaReporterFinishReportFunc,
    abort_report: SvnRaReporterAbortReportFunc,
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
enum SvnTxdeltaWindowT {}

#[cfg(git_svn_rs_libsvn_linked)]
enum SvnTxdeltaStreamT {}

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
struct SimplePromptBaton {
    prompt: *const Arc<dyn AuthPrompt>,
    no_auth_cache: bool,
}

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
type SvnAuthSimplePromptFunc = Option<
    unsafe extern "C" fn(
        *mut *mut svn_auth_cred_simple_t,
        *mut c_void,
        *const c_char,
        *const c_char,
        c_int,
        *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn prompt_simple_credentials(
    cred: *mut *mut svn_auth_cred_simple_t,
    baton: *mut c_void,
    realm: *const c_char,
    username: *const c_char,
    may_save: c_int,
    pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if cred.is_null() || baton.is_null() || pool.is_null() {
        return ptr::null_mut();
    }

    let prompt_baton = unsafe { &*(baton.cast::<SimplePromptBaton>()) };
    if prompt_baton.prompt.is_null() {
        unsafe {
            *cred = ptr::null_mut();
        }
        return ptr::null_mut();
    }
    let prompt = unsafe { &*prompt_baton.prompt };
    let realm = if realm.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(realm) }
                .to_string_lossy()
                .into_owned(),
        )
    };
    let default_username = if username.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(username) }
                .to_string_lossy()
                .into_owned(),
        )
    };
    let credentials = match prompt.simple(AuthRequest {
        realm,
        default_username,
        may_save: may_save != 0,
        no_auth_cache: prompt_baton.no_auth_cache,
    }) {
        Ok(credentials) => credentials,
        Err(_) => {
            unsafe {
                *cred = ptr::null_mut();
            }
            return ptr::null_mut();
        }
    };
    let username = match CString::new(credentials.username) {
        Ok(username) => username,
        Err(_) => {
            unsafe {
                *cred = ptr::null_mut();
            }
            return ptr::null_mut();
        }
    };
    let password = match CString::new(credentials.password) {
        Ok(password) => password,
        Err(_) => {
            unsafe {
                *cred = ptr::null_mut();
            }
            return ptr::null_mut();
        }
    };
    let raw_cred = unsafe { apr_palloc(pool, std::mem::size_of::<svn_auth_cred_simple_t>()) }
        as *mut svn_auth_cred_simple_t;
    if raw_cred.is_null() {
        unsafe {
            *cred = ptr::null_mut();
        }
        return ptr::null_mut();
    }
    unsafe {
        (*raw_cred).username = apr_pstrdup(pool, username.as_ptr());
        (*raw_cred).password = apr_pstrdup(pool, password.as_ptr());
        (*raw_cred).may_save = if credentials.may_save { 1 } else { 0 };
        *cred = raw_cred;
    }
    ptr::null_mut()
}

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
        properties_modified: changed_path.props_modified == 1,
        content_modified: changed_path.text_modified == 1,
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
unsafe fn dir_listing(
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

#[cfg(git_svn_rs_libsvn_linked)]
unsafe fn get_log(
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
        fill_file_details(session, pool, session_path, &mut revisions)?;
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
    source_path: &str,
    target_path: &str,
    editor: &mut dyn FetchEditor,
) -> Result<(), String> {
    let normalized_source_path = normalize_ra_path(source_path);
    let normalized_target_path = normalize_ra_path(target_path);
    editor.open_root(revision.revision)?;
    for changed_path in &revision.changed_paths {
        if !path_matches(&changed_path.path, &normalized_source_path) {
            continue;
        }
        let editor_path = remapped_editor_path(
            &changed_path.path,
            &normalized_source_path,
            &normalized_target_path,
        );
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
                    )?;
                    replay_directory_details(&editor_path, changed_path, editor, true)?
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
                    replay_file_details(&editor_path, changed_path, editor, true, true)?;
                }
            },
            ChangeAction::Modify => {
                if matches!(changed_path.kind, NodeKind::File | NodeKind::Symlink) {
                    replay_file_details(
                        &editor_path,
                        changed_path,
                        editor,
                        changed_path.properties_modified,
                        changed_path.content_modified,
                    )?;
                } else if changed_path.kind == NodeKind::Directory {
                    replay_directory_details(
                        &editor_path,
                        changed_path,
                        editor,
                        changed_path.properties_modified,
                    )?;
                }
            }
        }
    }
    editor.close_edit()
}

#[cfg(git_svn_rs_libsvn_linked)]
fn replay_directory_details(
    editor_path: &str,
    changed_path: &ChangedPath,
    editor: &mut dyn FetchEditor,
    include_properties: bool,
) -> Result<(), String> {
    if include_properties {
        for (name, value) in &changed_path.properties {
            editor.change_directory_prop(editor_path, name, Some(value))?;
        }
    }
    Ok(())
}

#[cfg(git_svn_rs_libsvn_linked)]
fn replay_file_details(
    editor_path: &str,
    changed_path: &ChangedPath,
    editor: &mut dyn FetchEditor,
    include_properties: bool,
    include_content: bool,
) -> Result<(), String> {
    if include_properties {
        for &name in SUPPORTED_FILE_PROPS_WITH_REMOVALS {
            editor.change_file_prop(
                editor_path,
                name,
                changed_path.properties.get(name).map(String::as_str),
            )?;
        }
        for (name, value) in &changed_path.properties {
            if !SUPPORTED_FILE_PROPS_WITH_REMOVALS.contains(&name.as_str()) {
                editor.change_file_prop(editor_path, name, Some(value))?;
            }
        }
    }
    if include_content && let Some(content) = &changed_path.content {
        editor.apply_textdelta(editor_path, content)?;
    }
    Ok(())
}

#[cfg(git_svn_rs_libsvn_linked)]
const SUPPORTED_FILE_PROPS_WITH_REMOVALS: &[&str] = &[
    "svn:executable",
    "svn:special",
    "svn:eol-style",
    "svn:mime-type",
    "svn:keywords",
    "svn:needs-lock",
];

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
fn remapped_editor_path(changed_path: &str, source_path: &str, target_path: &str) -> String {
    let changed_path = changed_path.trim_matches('/');
    let relative_path = if source_path.is_empty() {
        changed_path
    } else {
        changed_path
            .strip_prefix(source_path)
            .map(|path| path.trim_start_matches('/'))
            .unwrap_or_default()
    };

    if target_path.is_empty() {
        relative_path.to_string()
    } else if relative_path.is_empty() {
        target_path.to_string()
    } else {
        format!("{target_path}/{relative_path}")
    }
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

#[cfg(git_svn_rs_libsvn_linked)]
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

#[cfg(git_svn_rs_libsvn_linked)]
unsafe fn svn_call(error: *mut svn_error_t, context: &str) -> Result<(), String> {
    if error.is_null() {
        Ok(())
    } else {
        let detail = unsafe { svn_error_detail(error, context) };
        unsafe { svn_error_clear(error) };
        Err(format!("{context} failed: {detail}"))
    }
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe fn svn_error_detail(error: *mut svn_error_t, context: &str) -> String {
    if error.is_null() {
        return context.to_string();
    }

    let mut messages = Vec::new();
    let mut buffer = [0i8; 512];
    let best_message = unsafe { svn_err_best_message(error, buffer.as_mut_ptr(), buffer.len()) };
    if !best_message.is_null() {
        let message = unsafe { CStr::from_ptr(best_message) }
            .to_string_lossy()
            .into_owned();
        if !message.is_empty() {
            messages.push(message);
        }
    }

    let mut child = unsafe { (*error).child };
    while !child.is_null() {
        let message = unsafe { (*child).message };
        if !message.is_null() {
            let message = unsafe { CStr::from_ptr(message) }
                .to_string_lossy()
                .into_owned();
            if !message.is_empty() && !messages.iter().any(|existing| existing == &message) {
                messages.push(message);
            }
        }
        child = unsafe { (*child).child };
    }

    if messages.is_empty() {
        context.to_string()
    } else {
        messages.join(": ")
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
    fn apr_palloc(pool: *mut AprPoolT, size: usize) -> *mut c_void;
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
    #[allow(dead_code)]
    fn svn_stream_empty(pool: *mut AprPoolT) -> *mut SvnStreamT;
    #[allow(dead_code)]
    fn svn_txdelta_apply(
        source: *mut SvnStreamT,
        target: *mut SvnStreamT,
        result_digest: *mut c_char,
        error_info: *const c_char,
        pool: *mut AprPoolT,
        handler: *mut SvnTxdeltaWindowHandlerFunc,
        handler_baton: *mut *mut c_void,
    );
    fn svn_subr_version() -> *const svn_version_t;
    fn svn_err_best_message(
        error: *const svn_error_t,
        buffer: *mut c_char,
        buffer_size: usize,
    ) -> *const c_char;
    fn svn_error_clear(error: *mut svn_error_t);
    #[allow(dead_code)]
    fn svn_delta_default_editor(pool: *mut AprPoolT) -> *mut SvnDeltaEditorT;
    #[allow(dead_code)]
    fn svn_ra_do_update3(
        session: *mut SvnRaSessionT,
        reporter: *mut *const SvnRaReporter3T,
        report_baton: *mut *mut c_void,
        revision_to_update_to: c_long,
        update_target: *const c_char,
        depth: c_int,
        send_copyfrom_args: c_int,
        ignore_ancestry: c_int,
        update_editor: *const SvnDeltaEditorT,
        update_baton: *mut c_void,
        result_pool: *mut AprPoolT,
        scratch_pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    fn svn_auth_get_simple_provider2(
        provider: *mut *mut SvnAuthProviderObjectT,
        plaintext_prompt_func: SvnAuthPlaintextPromptFunc,
        prompt_baton: *mut c_void,
        pool: *mut AprPoolT,
    );
    fn svn_auth_get_simple_prompt_provider(
        provider: *mut *mut SvnAuthProviderObjectT,
        prompt_func: SvnAuthSimplePromptFunc,
        prompt_baton: *mut c_void,
        retry_limit: c_int,
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
    fn svn_ra_get_repos_root2(
        session: *mut SvnRaSessionT,
        url: *mut *const c_char,
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
            let session_path = self.session_repository_path();
            self.with_session(|session, pool| unsafe {
                get_log(session, pool, &session_path, &[], start, end)
            })
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
        #[cfg(git_svn_rs_libsvn_linked)]
        {
            self.repos_root
                .get_or_init(|| {
                    self.read_repos_root()
                        .unwrap_or_else(|_| self.url().to_string())
                })
                .as_str()
        }

        #[cfg(not(git_svn_rs_libsvn_linked))]
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
                let (entries, properties) = dir_listing(session, pool, path, revision_number)?;
                let entries = entries
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
                    properties,
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
            let session_path = self.session_repository_path();
            self.with_session(|session, pool| unsafe {
                get_log(session, pool, &session_path, paths, start, end)
            })
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
            self.replay_log_update(path, path, revision, editor)
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
        #[cfg(git_svn_rs_libsvn_linked)]
        {
            let source_path = switch_url_path_in_repository(self.url.as_deref(), switch_url)?;
            self.replay_log_update(&source_path, path, revision, editor)
        }

        #[cfg(not(git_svn_rs_libsvn_linked))]
        {
            let _ = switch_url;
            self.do_update(path, revision, editor)
        }
    }
}

#[cfg(git_svn_rs_libsvn_linked)]
fn switch_url_path_in_repository(
    repository_url: Option<&str>,
    switch_url: &str,
) -> Result<String, String> {
    url_path_in_repository(repository_url, switch_url, "switch URL")
}

#[cfg(git_svn_rs_libsvn_linked)]
fn url_path_in_repository(
    repository_url: Option<&str>,
    url: &str,
    url_label: &str,
) -> Result<String, String> {
    let repository_url = repository_url
        .ok_or_else(|| "libsvn backend requires an SVN repository URL".to_string())?;
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

#[cfg(git_svn_rs_libsvn_linked)]
fn join_ra_paths(base: &str, path: &str) -> String {
    let base = base.trim_matches('/');
    let path = path.trim_matches('/');
    match (base.is_empty(), path.is_empty()) {
        (true, true) => String::new(),
        (true, false) => path.to_string(),
        (false, true) => base.to_string(),
        (false, false) => format!("{base}/{path}"),
    }
}

#[cfg(git_svn_rs_libsvn_linked)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(git_svn_rs_libsvn_linked)]
    use std::fs;
    #[cfg(git_svn_rs_libsvn_linked)]
    use std::path::Path;
    #[cfg(git_svn_rs_libsvn_linked)]
    use std::process::Command;
    #[cfg(git_svn_rs_libsvn_linked)]
    use std::sync::Mutex;
    #[cfg(git_svn_rs_libsvn_linked)]
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[cfg(git_svn_rs_libsvn_linked)]
    static CLOSED_DIRECTORY_CALLBACKS: AtomicUsize = AtomicUsize::new(0);
    #[cfg(git_svn_rs_libsvn_linked)]
    static ADDED_FILE_CALLBACKS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    #[cfg(git_svn_rs_libsvn_linked)]
    static CLOSED_FILE_CALLBACKS: AtomicUsize = AtomicUsize::new(0);
    #[cfg(git_svn_rs_libsvn_linked)]
    static OPENED_FILE_CALLBACKS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    #[cfg(git_svn_rs_libsvn_linked)]
    static FILE_PROP_CALLBACKS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    #[cfg(git_svn_rs_libsvn_linked)]
    static DIR_PROP_CALLBACKS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    #[cfg(git_svn_rs_libsvn_linked)]
    static DELETE_ENTRY_CALLBACKS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    #[cfg(git_svn_rs_libsvn_linked)]
    static ADDED_DIRECTORY_CALLBACKS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    #[cfg(git_svn_rs_libsvn_linked)]
    static OPENED_DIRECTORY_CALLBACKS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    #[cfg(git_svn_rs_libsvn_linked)]
    static APPLY_TEXTDELTA_CALLBACKS: AtomicUsize = AtomicUsize::new(0);
    #[cfg(git_svn_rs_libsvn_linked)]
    static TEXTDELTA_APPLIED_TO_BUFFER: AtomicUsize = AtomicUsize::new(0);
    #[cfg(git_svn_rs_libsvn_linked)]
    static APPLY_TEXTDELTA_STREAM_CALLBACKS: AtomicUsize = AtomicUsize::new(0);
    #[cfg(git_svn_rs_libsvn_linked)]
    static TEXTDELTA_WINDOWS: AtomicUsize = AtomicUsize::new(0);
    #[cfg(git_svn_rs_libsvn_linked)]
    static TEXTDELTA_DONE_WINDOWS: AtomicUsize = AtomicUsize::new(0);
    #[cfg(git_svn_rs_libsvn_linked)]
    static ABSENT_DIRECTORY_CALLBACKS: AtomicUsize = AtomicUsize::new(0);
    #[cfg(git_svn_rs_libsvn_linked)]
    static ABSENT_FILE_CALLBACKS: AtomicUsize = AtomicUsize::new(0);
    #[cfg(git_svn_rs_libsvn_linked)]
    static ABORT_EDIT_CALLBACKS: AtomicUsize = AtomicUsize::new(0);
    #[cfg(git_svn_rs_libsvn_linked)]
    static NATIVE_UPDATE_CALLBACK_LOCK: Mutex<()> = Mutex::new(());

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
        CLOSED_DIRECTORY_CALLBACKS.store(0, Ordering::SeqCst);

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
        assert!(CLOSED_DIRECTORY_CALLBACKS.load(Ordering::SeqCst) > 0);
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
        ADDED_FILE_CALLBACKS.lock().unwrap().clear();
        CLOSED_FILE_CALLBACKS.store(0, Ordering::SeqCst);

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

        assert_eq!(
            ADDED_FILE_CALLBACKS.lock().unwrap().as_slice(),
            ["trunk/file.txt"]
        );
        assert_eq!(CLOSED_FILE_CALLBACKS.load(Ordering::SeqCst), 1);
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
        APPLY_TEXTDELTA_CALLBACKS.store(0, Ordering::SeqCst);
        TEXTDELTA_WINDOWS.store(0, Ordering::SeqCst);
        TEXTDELTA_DONE_WINDOWS.store(0, Ordering::SeqCst);

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

        assert_eq!(APPLY_TEXTDELTA_CALLBACKS.load(Ordering::SeqCst), 1);
        assert!(TEXTDELTA_WINDOWS.load(Ordering::SeqCst) > 0);
        assert_eq!(TEXTDELTA_DONE_WINDOWS.load(Ordering::SeqCst), 1);
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
        TEXTDELTA_APPLIED_TO_BUFFER.store(0, Ordering::SeqCst);

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

        assert_eq!(TEXTDELTA_APPLIED_TO_BUFFER.load(Ordering::SeqCst), 1);
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
            editor.events.contains(
                &"change_directory_prop:trunk/subdir:svn:ignore=nested-target\n".to_string()
            ),
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
        APPLY_TEXTDELTA_STREAM_CALLBACKS.store(0, Ordering::SeqCst);

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
        assert_eq!(APPLY_TEXTDELTA_STREAM_CALLBACKS.load(Ordering::SeqCst), 1);
    }

    #[cfg(git_svn_rs_libsvn_linked)]
    #[test]
    fn default_delta_editor_accepts_patched_absent_and_abort_callbacks() {
        AprRuntime::initialize().unwrap();
        let apr = AprRuntime;
        let pool = apr.create_pool().unwrap();
        let editor = unsafe { svn_delta_default_editor(pool.as_ptr()) };
        ABSENT_DIRECTORY_CALLBACKS.store(0, Ordering::SeqCst);
        ABSENT_FILE_CALLBACKS.store(0, Ordering::SeqCst);
        ABORT_EDIT_CALLBACKS.store(0, Ordering::SeqCst);

        assert!(!editor.is_null());
        let error = unsafe {
            (*editor).absent_directory = Some(record_absent_directory);
            (*editor).absent_file = Some(record_absent_file);
            (*editor).abort_edit = Some(record_abort_edit);

            let absent_path = CString::new("trunk/missing").unwrap();
            let absent_directory = (*editor).absent_directory.unwrap();
            let absent_error =
                absent_directory(absent_path.as_ptr(), ptr::null_mut(), pool.as_ptr());
            assert!(absent_error.is_null());

            let absent_file = (*editor).absent_file.unwrap();
            let absent_error = absent_file(absent_path.as_ptr(), ptr::null_mut(), pool.as_ptr());
            assert!(absent_error.is_null());

            let abort_edit = (*editor).abort_edit.unwrap();
            abort_edit(ptr::null_mut(), pool.as_ptr())
        };

        assert!(error.is_null());
        assert_eq!(ABSENT_DIRECTORY_CALLBACKS.load(Ordering::SeqCst), 1);
        assert_eq!(ABSENT_FILE_CALLBACKS.load(Ordering::SeqCst), 1);
        assert_eq!(ABORT_EDIT_CALLBACKS.load(Ordering::SeqCst), 1);
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
        OPENED_FILE_CALLBACKS.lock().unwrap().clear();

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

        assert_eq!(
            OPENED_FILE_CALLBACKS.lock().unwrap().as_slice(),
            ["trunk/file.txt@2"]
        );
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
        FILE_PROP_CALLBACKS.lock().unwrap().clear();

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
            FILE_PROP_CALLBACKS
                .lock()
                .unwrap()
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
        DIR_PROP_CALLBACKS.lock().unwrap().clear();

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
            DIR_PROP_CALLBACKS
                .lock()
                .unwrap()
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
        DELETE_ENTRY_CALLBACKS.lock().unwrap().clear();

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

        assert_eq!(
            DELETE_ENTRY_CALLBACKS.lock().unwrap().as_slice(),
            ["trunk/file.txt@6"]
        );
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
        ADDED_DIRECTORY_CALLBACKS.lock().unwrap().clear();

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

        assert_eq!(
            ADDED_DIRECTORY_CALLBACKS.lock().unwrap().as_slice(),
            ["trunk/subdir"]
        );
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
        OPENED_DIRECTORY_CALLBACKS.lock().unwrap().clear();

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
            OPENED_DIRECTORY_CALLBACKS
                .lock()
                .unwrap()
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
        let _callback_lock = NATIVE_UPDATE_CALLBACK_LOCK.lock().unwrap();
        backend.with_session(|session, pool| unsafe {
            let editor = svn_delta_default_editor(pool);
            assert!(!editor.is_null());
            configure_editor(editor);

            let mut reporter: *const SvnRaReporter3T = ptr::null();
            let mut report_baton: *mut c_void = ptr::null_mut();
            let target = CString::new("trunk").unwrap();
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

            assert!(!reporter.is_null());
            let set_path = (*reporter)
                .set_path
                .ok_or_else(|| "libsvn returned a reporter without set_path".to_string())?;
            let finish_report = (*reporter)
                .finish_report
                .ok_or_else(|| "libsvn returned a reporter without finish_report".to_string())?;

            let empty_path = CString::new("").unwrap();
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
        let _callback_lock = NATIVE_UPDATE_CALLBACK_LOCK.lock().unwrap();
        backend.with_session(|session, pool| unsafe {
            let editor = svn_delta_default_editor(pool);
            assert!(!editor.is_null());

            let mut baton = FetchEditorUpdateBaton {
                editor: fetch_editor,
                active_directory_paths: vec![target.to_string()],
                active_file_path: None,
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

            assert!(!reporter.is_null());
            let set_path = (*reporter)
                .set_path
                .ok_or_else(|| "libsvn returned a reporter without set_path".to_string())?;
            let finish_report = (*reporter)
                .finish_report
                .ok_or_else(|| "libsvn returned a reporter without finish_report".to_string())?;

            let empty_path = CString::new("").unwrap();
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
        textdelta_buffer: *mut svn_stringbuf_t,
    }

    #[cfg(git_svn_rs_libsvn_linked)]
    struct FetchEditorUpdateBaton<'a> {
        editor: &'a mut dyn FetchEditor,
        active_directory_paths: Vec<String>,
        active_file_path: Option<String>,
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

        fn add_directory(
            &mut self,
            path: &str,
            _copy_from: Option<(&str, u32)>,
        ) -> Result<(), String> {
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
            baton.active_file_path = Some(
                unsafe { CStr::from_ptr(path) }
                    .to_string_lossy()
                    .into_owned(),
            );
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
            baton.error = Some("libsvn textdelta callback had no active file buffer".to_string());
            return ptr::null_mut();
        }
        let source_stream = unsafe { svn_stream_empty(result_pool) };
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
        let baton = unsafe { &mut *(edit_baton as *mut RecordingEditorBaton) };
        baton.root_opened = true;
        baton.root_base_revision = base_revision;
        unsafe {
            *root_baton = edit_baton;
        }
        ptr::null_mut()
    }

    #[cfg(git_svn_rs_libsvn_linked)]
    unsafe extern "C" fn record_close_directory(
        dir_baton: *mut c_void,
        _pool: *mut AprPoolT,
    ) -> *mut svn_error_t {
        CLOSED_DIRECTORY_CALLBACKS.fetch_add(1, Ordering::SeqCst);
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
        _parent_baton: *mut c_void,
        _pool: *mut AprPoolT,
    ) -> *mut svn_error_t {
        let path = unsafe { CStr::from_ptr(path) }.to_string_lossy();
        DELETE_ENTRY_CALLBACKS
            .lock()
            .unwrap()
            .push(format!("{path}@{revision}"));
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
        let path = unsafe { CStr::from_ptr(path) }
            .to_string_lossy()
            .into_owned();
        ADDED_DIRECTORY_CALLBACKS.lock().unwrap().push(path);
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
        let path = unsafe { CStr::from_ptr(path) }.to_string_lossy();
        OPENED_DIRECTORY_CALLBACKS
            .lock()
            .unwrap()
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
        let path = unsafe { CStr::from_ptr(path) }
            .to_string_lossy()
            .into_owned();
        ADDED_FILE_CALLBACKS.lock().unwrap().push(path.clone());
        if !parent_baton.is_null() {
            let baton = unsafe { &mut *(parent_baton as *mut RecordingEditorBaton) };
            baton.added_files.push(path);
        }
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
        CLOSED_FILE_CALLBACKS.fetch_add(1, Ordering::SeqCst);
        if !file_baton.is_null() {
            let baton = unsafe { &mut *(file_baton as *mut RecordingEditorBaton) };
            baton.closed_files += 1;
        }
        ptr::null_mut()
    }

    #[cfg(git_svn_rs_libsvn_linked)]
    unsafe extern "C" fn record_open_file(
        path: *const c_char,
        _parent_baton: *mut c_void,
        base_revision: c_long,
        _file_pool: *mut AprPoolT,
        file_baton: *mut *mut c_void,
    ) -> *mut svn_error_t {
        let path = unsafe { CStr::from_ptr(path) }.to_string_lossy();
        OPENED_FILE_CALLBACKS
            .lock()
            .unwrap()
            .push(format!("{path}@{base_revision}"));
        unsafe {
            *file_baton = ptr::null_mut();
        }
        ptr::null_mut()
    }

    #[cfg(git_svn_rs_libsvn_linked)]
    unsafe extern "C" fn record_change_file_prop(
        _file_baton: *mut c_void,
        name: *const c_char,
        value: *const svn_string_t,
        _pool: *mut AprPoolT,
    ) -> *mut svn_error_t {
        let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
        let value = if value.is_null() {
            String::new()
        } else {
            let bytes = unsafe { slice::from_raw_parts((*value).data.cast::<u8>(), (*value).len) };
            String::from_utf8_lossy(bytes).into_owned()
        };
        FILE_PROP_CALLBACKS
            .lock()
            .unwrap()
            .push(format!("{name}={value}"));
        ptr::null_mut()
    }

    #[cfg(git_svn_rs_libsvn_linked)]
    unsafe extern "C" fn record_change_dir_prop(
        _dir_baton: *mut c_void,
        name: *const c_char,
        value: *const svn_string_t,
        _pool: *mut AprPoolT,
    ) -> *mut svn_error_t {
        let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
        let value = if value.is_null() {
            String::new()
        } else {
            let bytes = unsafe { slice::from_raw_parts((*value).data.cast::<u8>(), (*value).len) };
            String::from_utf8_lossy(bytes).into_owned()
        };
        DIR_PROP_CALLBACKS
            .lock()
            .unwrap()
            .push(format!("{name}={value}"));
        ptr::null_mut()
    }

    #[cfg(git_svn_rs_libsvn_linked)]
    unsafe extern "C" fn record_apply_textdelta(
        _file_baton: *mut c_void,
        _base_checksum: *const c_char,
        _result_pool: *mut AprPoolT,
        handler: *mut SvnTxdeltaWindowHandlerFunc,
        handler_baton: *mut *mut c_void,
    ) -> *mut svn_error_t {
        APPLY_TEXTDELTA_CALLBACKS.fetch_add(1, Ordering::SeqCst);
        unsafe {
            *handler = Some(record_textdelta_window);
            *handler_baton = ptr::null_mut();
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
        assert!(!file_baton.is_null());
        assert!(!result_pool.is_null());
        let baton = unsafe { &mut *(file_baton as *mut RecordingEditorBaton) };
        assert!(!baton.textdelta_buffer.is_null());
        let source_stream = unsafe { svn_stream_empty(result_pool) };
        assert!(!source_stream.is_null());
        let target_stream =
            unsafe { svn_stream_from_stringbuf(baton.textdelta_buffer, result_pool) };
        assert!(!target_stream.is_null());
        TEXTDELTA_APPLIED_TO_BUFFER.fetch_add(1, Ordering::SeqCst);
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
        assert!(open_func.is_some());
        APPLY_TEXTDELTA_STREAM_CALLBACKS.fetch_add(1, Ordering::SeqCst);
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
        _parent_baton: *mut c_void,
        _pool: *mut AprPoolT,
    ) -> *mut svn_error_t {
        ABSENT_DIRECTORY_CALLBACKS.fetch_add(1, Ordering::SeqCst);
        ptr::null_mut()
    }

    #[cfg(git_svn_rs_libsvn_linked)]
    unsafe extern "C" fn record_absent_file(
        _path: *const c_char,
        _parent_baton: *mut c_void,
        _pool: *mut AprPoolT,
    ) -> *mut svn_error_t {
        ABSENT_FILE_CALLBACKS.fetch_add(1, Ordering::SeqCst);
        ptr::null_mut()
    }

    #[cfg(git_svn_rs_libsvn_linked)]
    unsafe extern "C" fn record_abort_edit(
        _edit_baton: *mut c_void,
        _pool: *mut AprPoolT,
    ) -> *mut svn_error_t {
        ABORT_EDIT_CALLBACKS.fetch_add(1, Ordering::SeqCst);
        ptr::null_mut()
    }

    #[cfg(git_svn_rs_libsvn_linked)]
    unsafe extern "C" fn record_textdelta_window(
        window: *mut SvnTxdeltaWindowT,
        _baton: *mut c_void,
    ) -> *mut svn_error_t {
        if window.is_null() {
            TEXTDELTA_DONE_WINDOWS.fetch_add(1, Ordering::SeqCst);
        } else {
            TEXTDELTA_WINDOWS.fetch_add(1, Ordering::SeqCst);
        }
        ptr::null_mut()
    }

    #[cfg(git_svn_rs_libsvn_linked)]
    fn create_minimal_svn_repository() -> Result<(tempfile::TempDir, String), String> {
        if !command_succeeds("svnadmin", &["--version"]) || !command_succeeds("svn", &["--version"])
        {
            return Err("svnadmin and svn are required".to_string());
        }

        let tmp = tempfile::tempdir().map_err(|error| error.to_string())?;
        let repo = tmp.path().join("repo");
        let wc = tmp.path().join("wc");
        run(tmp.path(), "svnadmin", &["create", path_arg(&repo)?])?;
        let repo_url =
            url::Url::from_directory_path(repo.canonicalize().map_err(|e| e.to_string())?)
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
}
