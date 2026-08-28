use super::LibSvnBackend;
use super::ffi::{
    AprPoolT, SvnAuthBatonT, SvnAuthProviderObjectT, apr_array_header_t, apr_array_make,
    apr_array_push, apr_palloc, apr_pstrdup, svn_auth_cred_simple_t,
    svn_auth_get_simple_prompt_provider, svn_auth_get_simple_provider2,
    svn_auth_get_username_provider, svn_auth_open, svn_auth_set_parameter, svn_error_t,
};
use super::runtime::{callback_error_message, pool_c_string};
use crate::svn::auth::{AuthPrompt, AuthRequest};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::sync::Arc;

const SVN_AUTH_PARAM_DEFAULT_USERNAME: &[u8] = b"svn:auth:username\0";
const SVN_AUTH_PARAM_DEFAULT_PASSWORD: &[u8] = b"svn:auth:password\0";
const SVN_AUTH_PARAM_NO_AUTH_CACHE: &[u8] = b"svn:auth:no-auth-cache\0";
const SVN_AUTH_PRESENT_VALUE: &[u8] = b"1\0";

impl LibSvnBackend {
    pub(super) unsafe fn auth_baton(
        &self,
        pool: *mut AprPoolT,
    ) -> Result<*mut SvnAuthBatonT, String> {
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

pub(super) struct SimplePromptBaton {
    pub(super) prompt: *const Arc<dyn AuthPrompt>,
    pub(super) no_auth_cache: bool,
}

pub(super) unsafe extern "C" fn prompt_simple_credentials(
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

unsafe fn push_auth_provider(
    array: *mut apr_array_header_t,
    provider: *mut SvnAuthProviderObjectT,
) {
    unsafe {
        *(apr_array_push(array) as *mut *mut SvnAuthProviderObjectT) = provider;
    }
}
