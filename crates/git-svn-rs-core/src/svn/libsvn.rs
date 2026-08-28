#![cfg_attr(
    all(not(windows), target_pointer_width = "64"),
    allow(clippy::unnecessary_fallible_conversions)
)]

use super::auth::AuthPrompt;
use super::editor::FetchEditor;
#[cfg(git_svn_rs_libsvn_linked)]
use super::ra::DirEntry;
use super::ra::{DirListing, RaSession, SvnNodeKind, UpdateRequest};
use super::{RevisionEvent, SvnBackend};
use crate::config::SvnRemoteConfig;
use std::collections::BTreeMap;
#[cfg(git_svn_rs_libsvn_linked)]
use std::ffi::{CStr, CString};
#[cfg(git_svn_rs_libsvn_linked)]
use std::os::raw::{c_int, c_long};
#[cfg(git_svn_rs_libsvn_linked)]
use std::ptr;
use std::sync::Arc;
#[cfg(git_svn_rs_libsvn_linked)]
use std::sync::OnceLock;

#[cfg(git_svn_rs_libsvn_linked)]
mod ffi;
#[cfg(git_svn_rs_libsvn_linked)]
use ffi::*;
#[cfg(git_svn_rs_libsvn_linked)]
mod auth;
#[cfg(git_svn_rs_libsvn_linked)]
mod native_delta;
#[cfg(git_svn_rs_libsvn_linked)]
mod ra;
#[cfg(git_svn_rs_libsvn_linked)]
use ra::{
    dir_listing, get_log, svn_node_kind_from_change_kind, svn_node_kind_from_raw,
    svn_property_bytes, switch_url_path_in_repository,
};
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

#[cfg(test)]
mod tests;
