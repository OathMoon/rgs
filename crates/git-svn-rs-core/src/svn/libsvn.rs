use super::{RevisionEvent, SvnBackend};
#[cfg(git_svn_rs_libsvn_linked)]
use std::ffi::CStr;
#[cfg(git_svn_rs_libsvn_linked)]
use std::os::raw::c_char;

pub const LIBSVN_NOT_LINKED_MESSAGE: &str =
    "libsvn backend is enabled but not linked: no libsvn FFI bindings are compiled into this build";
pub const LIBSVN_LINKED_PROBE_MESSAGE: &str =
    "libsvn link probe succeeded via vcpkg; backend API calls are not implemented yet";
pub const LIBSVN_NOT_IMPLEMENTED_MESSAGE: &str =
    "libsvn backend is linked, but native libsvn API calls are not implemented yet";

#[derive(Debug, Default)]
pub struct LibSvnBackend;

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
        Self
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
            return Ok(format!(
                "{}.{}.{}{}",
                version.major, version.minor, version.patch, tag
            ));
        }

        #[cfg(not(git_svn_rs_libsvn_linked))]
        Err(Self::unavailable_message().to_string())
    }

    fn unavailable_message() -> &'static str {
        if Self::availability().link_status == LibSvnLinkStatus::Linked {
            LIBSVN_NOT_IMPLEMENTED_MESSAGE
        } else {
            LIBSVN_NOT_LINKED_MESSAGE
        }
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
unsafe extern "C" {
    fn svn_subr_version() -> *const svn_version_t;
}

impl SvnBackend for LibSvnBackend {
    fn uuid(&self) -> Result<String, String> {
        Err(Self::unavailable_message().to_string())
    }

    fn latest_revnum(&self) -> Result<u32, String> {
        Err(Self::unavailable_message().to_string())
    }

    fn log(&self, _start: u32, _end: u32) -> Result<Vec<RevisionEvent>, String> {
        Err(Self::unavailable_message().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_type_is_constructible_without_ffi() {
        let backend = LibSvnBackend::new();

        assert_eq!(
            backend.uuid().unwrap_err(),
            LibSvnBackend::unavailable_message()
        );
    }
}
