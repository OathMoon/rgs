#![cfg_attr(
    all(not(windows), target_pointer_width = "64"),
    allow(clippy::unnecessary_fallible_conversions)
)]

use super::auth::AuthPrompt;
#[cfg(git_svn_rs_libsvn_linked)]
use super::auth::AuthRequest;
use super::editor::FetchEditor;
#[cfg(git_svn_rs_libsvn_linked)]
use super::ra::DirEntry;
use super::ra::{DirListing, RaSession, SvnNodeKind, UpdateRequest};
#[cfg(git_svn_rs_libsvn_linked)]
use super::{ChangeAction, ChangedPath, NodeKind};
use super::{RevisionEvent, SvnBackend};
use crate::config::SvnRemoteConfig;
use std::collections::BTreeMap;
#[cfg(git_svn_rs_libsvn_linked)]
use std::collections::BTreeSet;
#[cfg(git_svn_rs_libsvn_linked)]
use std::ffi::{CStr, CString};
#[cfg(git_svn_rs_libsvn_linked)]
use std::os::raw::{c_char, c_int, c_long, c_uint, c_void};
#[cfg(git_svn_rs_libsvn_linked)]
use std::panic::{AssertUnwindSafe, catch_unwind};
#[cfg(git_svn_rs_libsvn_linked)]
use std::ptr;
#[cfg(git_svn_rs_libsvn_linked)]
use std::slice;
use std::sync::Arc;
#[cfg(git_svn_rs_libsvn_linked)]
use std::sync::OnceLock;

#[cfg(git_svn_rs_libsvn_linked)]
mod ffi;
#[cfg(git_svn_rs_libsvn_linked)]
use ffi::*;
#[cfg(git_svn_rs_libsvn_linked)]
mod native_delta;
#[cfg(git_svn_rs_libsvn_linked)]
mod runtime;
#[cfg(git_svn_rs_libsvn_linked)]
use runtime::*;

pub const LIBSVN_NOT_LINKED_MESSAGE: &str =
    "libsvn backend is enabled but not linked: no libsvn FFI bindings are compiled into this build";
pub const LIBSVN_LINKED_PROBE_MESSAGE: &str =
    "libsvn platform link probe succeeded; native backend API calls are available";
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

    #[cfg(all(test, feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
    pub(crate) fn configured_username(&self) -> Option<&str> {
        self.username.as_deref()
    }

    #[cfg(all(test, feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
    pub(crate) fn configured_config_dir(&self) -> Option<&str> {
        self.config_dir.as_deref()
    }

    #[cfg(all(test, feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
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

    #[cfg(git_svn_rs_libsvn_linked)]
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
struct SimplePromptBaton {
    prompt: *const Arc<dyn AuthPrompt>,
    no_auth_cache: bool,
}

#[cfg(git_svn_rs_libsvn_linked)]
unsafe extern "C" fn prompt_simple_credentials(
    cred: *mut *mut svn_auth_cred_simple_t,
    baton: *mut c_void,
    realm: *const c_char,
    username: *const c_char,
    may_save: c_int,
    pool: *mut AprPoolT,
) -> *mut svn_error_t {
    if !cred.is_null() {
        unsafe {
            *cred = ptr::null_mut();
        }
    }
    if cred.is_null() || baton.is_null() || pool.is_null() {
        return unsafe {
            callback_error_message("libsvn credential callback had null input or pool")
        };
    }

    let prompt_baton = unsafe { &*(baton.cast::<SimplePromptBaton>()) };
    if prompt_baton.prompt.is_null() {
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
    let credentials = match catch_unwind(AssertUnwindSafe(|| {
        prompt.simple(AuthRequest {
            realm,
            default_username,
            may_save: may_save != 0,
            no_auth_cache: prompt_baton.no_auth_cache,
        })
    })) {
        Ok(Ok(credentials)) => credentials,
        Ok(Err(_)) | Err(_) => return ptr::null_mut(),
    };
    let username = match CString::new(credentials.username) {
        Ok(username) => username,
        Err(_) => return ptr::null_mut(),
    };
    let password = match CString::new(credentials.password) {
        Ok(password) => password,
        Err(_) => return ptr::null_mut(),
    };
    let raw_cred = unsafe { apr_palloc(pool, std::mem::size_of::<svn_auth_cred_simple_t>()) }
        as *mut svn_auth_cred_simple_t;
    if raw_cred.is_null() {
        return unsafe { callback_error_message("APR failed to allocate SVN credentials") };
    }
    let username = unsafe { apr_pstrdup(pool, username.as_ptr()) };
    let password = unsafe { apr_pstrdup(pool, password.as_ptr()) };
    if username.is_null() || password.is_null() {
        return unsafe { callback_error_message("APR failed to copy SVN credentials") };
    }
    unsafe {
        (*raw_cred).username = username;
        (*raw_cred).password = password;
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
fn editor_copy_from(path: &Option<String>, revision: Option<u32>) -> Option<(String, u32)> {
    path.as_ref()
        .zip(revision)
        .map(|(path, revision)| (editor_path(path), revision))
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
unsafe fn svn_property_bytes(props: *mut AprHashT) -> BTreeMap<String, Vec<u8>> {
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

    fn rev_properties(&self, revision: u32) -> Result<BTreeMap<String, Vec<u8>>, String> {
        #[cfg(git_svn_rs_libsvn_linked)]
        {
            self.with_session(|session, pool| unsafe {
                let revision_number: c_long = revision
                    .try_into()
                    .map_err(|_| format!("SVN revision {revision} does not fit in svn_revnum_t"))?;
                let mut properties = ptr::null_mut();
                svn_call(
                    svn_ra_rev_proplist(session, revision_number, &mut properties, pool),
                    "svn_ra_rev_proplist",
                )?;
                Ok(svn_property_bytes(properties))
            })
        }

        #[cfg(not(git_svn_rs_libsvn_linked))]
        {
            let _ = revision;
            Err(Self::unavailable_message().to_string())
        }
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
        self.do_update_from(
            path,
            UpdateRequest {
                target_revision: revision,
                base_revision: revision.checked_sub(1),
            },
            editor,
        )
    }

    fn do_update_from(
        &self,
        path: &str,
        request: UpdateRequest,
        editor: &mut dyn FetchEditor,
    ) -> Result<(), String> {
        #[cfg(git_svn_rs_libsvn_linked)]
        {
            native_delta::drive_update(self, path, request, editor)
        }

        #[cfg(not(git_svn_rs_libsvn_linked))]
        {
            let _ = (path, request, editor);
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
            native_delta::drive_switch(
                self,
                path,
                UpdateRequest {
                    target_revision: revision,
                    base_revision: None,
                },
                switch_url,
                &source_path,
                editor,
            )
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

#[cfg(git_svn_rs_libsvn_linked)]
fn canonical_libsvn_url(url: &str) -> String {
    url.replace("%40", "@")
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

#[cfg(git_svn_rs_libsvn_linked)]
fn repository_path_is_within(session_path: &str, repository_path: &str) -> bool {
    let session_path = session_path.trim_matches('/');
    let repository_path = repository_path.trim_matches('/');
    repository_path == session_path || repository_path.starts_with(&format!("{session_path}/"))
}

#[cfg(git_svn_rs_libsvn_linked)]
fn session_editor_path(session_path: &str, repository_path: &str) -> String {
    let relative = session_relative_path(session_path, repository_path);
    if relative.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", relative.trim_start_matches('/'))
    }
}

#[cfg(test)]
mod tests;
