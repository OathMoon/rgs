use super::{RevisionEvent, SvnBackend};
#[cfg(git_svn_rs_libsvn_linked)]
use std::ffi::{CStr, CString};
#[cfg(git_svn_rs_libsvn_linked)]
use std::os::raw::{c_char, c_int, c_long, c_void};
#[cfg(git_svn_rs_libsvn_linked)]
use std::ptr;
#[cfg(git_svn_rs_libsvn_linked)]
use std::sync::OnceLock;

pub const LIBSVN_NOT_LINKED_MESSAGE: &str =
    "libsvn backend is enabled but not linked: no libsvn FFI bindings are compiled into this build";
pub const LIBSVN_LINKED_PROBE_MESSAGE: &str =
    "libsvn link probe succeeded via vcpkg; backend API calls are not implemented yet";
pub const LIBSVN_NOT_IMPLEMENTED_MESSAGE: &str =
    "libsvn backend is linked, but native libsvn API calls are not implemented yet";

#[derive(Debug, Default)]
pub struct LibSvnBackend {
    #[cfg_attr(not(git_svn_rs_libsvn_linked), allow(dead_code))]
    url: Option<String>,
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
        Self { url: None }
    }

    pub fn for_url(url: impl Into<String>) -> Self {
        Self {
            url: Some(url.into()),
        }
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
                    ptr::null_mut(),
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
enum AprPoolT {}

#[cfg(git_svn_rs_libsvn_linked)]
enum SvnRaCallbacks2T {}

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
    fn svn_subr_version() -> *const svn_version_t;
    fn svn_err_best_message(
        error: *const svn_error_t,
        buffer: *mut c_char,
        buffer_size: usize,
    ) -> *const c_char;
    fn svn_error_clear(error: *mut svn_error_t);
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
        config: *mut c_void,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    fn svn_ra_get_latest_revnum(
        session: *mut SvnRaSessionT,
        latest_revnum: *mut c_long,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    fn svn_ra_get_uuid2(
        session: *mut SvnRaSessionT,
        uuid: *mut *const c_char,
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
