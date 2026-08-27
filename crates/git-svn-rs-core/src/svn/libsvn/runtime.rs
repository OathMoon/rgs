use super::ffi::{
    AprPoolT, apr_initialize, apr_pool_create_ex, apr_pool_destroy, apr_pstrdup,
    svn_err_best_message, svn_error_clear, svn_error_create, svn_error_t,
};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;
use std::sync::OnceLock;

const SVN_ERR_CANCELLED: i32 = 200_015;

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) struct AprRuntime;

#[cfg(git_svn_rs_libsvn_linked)]
impl AprRuntime {
    pub(super) fn initialize() -> Result<(), String> {
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

    pub(super) fn create_pool(&self) -> Result<AprPool, String> {
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
pub(super) struct AprPool {
    pool: *mut AprPoolT,
}

#[cfg(git_svn_rs_libsvn_linked)]
impl AprPool {
    pub(super) fn as_ptr(&self) -> *mut AprPoolT {
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
pub(super) unsafe fn callback_error_message(message: &str) -> *mut svn_error_t {
    let message = CString::new(message);
    let message_ptr = match message.as_ref() {
        Ok(message) => message.as_ptr(),
        Err(_) => c"libsvn callback failed".as_ptr(),
    };
    unsafe { svn_error_create(SVN_ERR_CANCELLED, ptr::null_mut(), message_ptr) }
}

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) fn pool_c_string(
    pool: *mut AprPoolT,
    value: &str,
    label: &str,
) -> Result<*const c_char, String> {
    let value = CString::new(value).map_err(|_| format!("{label} contains NUL"))?;
    let value = unsafe { apr_pstrdup(pool, value.as_ptr()) };
    if value.is_null() {
        Err(format!("APR failed to allocate {label}"))
    } else {
        Ok(value)
    }
}

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) unsafe fn svn_call(error: *mut svn_error_t, context: &str) -> Result<(), String> {
    if error.is_null() {
        Ok(())
    } else {
        let detail = unsafe { svn_error_detail(error, context) };
        unsafe { svn_error_clear(error) };
        Err(format!("{context} failed: {detail}"))
    }
}

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) unsafe fn svn_error_detail(error: *mut svn_error_t, context: &str) -> String {
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
