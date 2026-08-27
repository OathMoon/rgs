use std::os::raw::{c_char, c_int, c_long, c_uint, c_void};

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
pub(super) struct svn_version_t {
    pub(super) major: i32,
    pub(super) minor: i32,
    pub(super) patch: i32,
    pub(super) tag: *const c_char,
}

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
pub(super) struct svn_error_t {
    pub(super) apr_err: c_int,
    pub(super) message: *const c_char,
    pub(super) child: *mut svn_error_t,
    pub(super) pool: *mut AprPoolT,
    pub(super) file: *const c_char,
    pub(super) line: c_long,
}

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
pub(super) struct svn_string_t {
    pub(super) data: *const c_char,
    pub(super) len: usize,
}

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
pub(super) struct svn_stringbuf_t {
    pub(super) pool: *mut AprPoolT,
    pub(super) data: *mut c_char,
    pub(super) len: usize,
    pub(super) blocksize: usize,
}

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
pub(super) struct svn_auth_cred_simple_t {
    pub(super) username: *const c_char,
    pub(super) password: *const c_char,
    pub(super) may_save: c_int,
}

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnDeltaSetTargetRevisionFunc = Option<
    unsafe extern "C" fn(
        edit_baton: *mut c_void,
        target_revision: c_long,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnDeltaCloseEditFunc =
    Option<unsafe extern "C" fn(edit_baton: *mut c_void, pool: *mut AprPoolT) -> *mut svn_error_t>;

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnDeltaOpenRootFunc = Option<
    unsafe extern "C" fn(
        edit_baton: *mut c_void,
        base_revision: c_long,
        dir_pool: *mut AprPoolT,
        root_baton: *mut *mut c_void,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnDeltaDeleteEntryFunc = Option<
    unsafe extern "C" fn(
        path: *const c_char,
        revision: c_long,
        parent_baton: *mut c_void,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnDeltaCloseDirectoryFunc =
    Option<unsafe extern "C" fn(dir_baton: *mut c_void, pool: *mut AprPoolT) -> *mut svn_error_t>;

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnDeltaAbsentDirectoryFunc = Option<
    unsafe extern "C" fn(
        path: *const c_char,
        parent_baton: *mut c_void,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnDeltaAddDirectoryFunc = Option<
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
pub(super) type SvnDeltaOpenDirectoryFunc = Option<
    unsafe extern "C" fn(
        path: *const c_char,
        parent_baton: *mut c_void,
        base_revision: c_long,
        dir_pool: *mut AprPoolT,
        child_baton: *mut *mut c_void,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnDeltaAddFileFunc = Option<
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
pub(super) type SvnDeltaOpenFileFunc = Option<
    unsafe extern "C" fn(
        path: *const c_char,
        parent_baton: *mut c_void,
        base_revision: c_long,
        file_pool: *mut AprPoolT,
        file_baton: *mut *mut c_void,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnDeltaAbsentFileFunc = Option<
    unsafe extern "C" fn(
        path: *const c_char,
        parent_baton: *mut c_void,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnDeltaCloseFileFunc = Option<
    unsafe extern "C" fn(
        file_baton: *mut c_void,
        text_checksum: *const c_char,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnTxdeltaWindowHandlerFunc = Option<
    unsafe extern "C" fn(window: *mut SvnTxdeltaWindowT, baton: *mut c_void) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnDeltaApplyTextdeltaFunc = Option<
    unsafe extern "C" fn(
        file_baton: *mut c_void,
        base_checksum: *const c_char,
        result_pool: *mut AprPoolT,
        handler: *mut SvnTxdeltaWindowHandlerFunc,
        handler_baton: *mut *mut c_void,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnTxdeltaStreamOpenFunc = Option<
    unsafe extern "C" fn(
        txdelta_stream: *mut *mut SvnTxdeltaStreamT,
        baton: *mut c_void,
        result_pool: *mut AprPoolT,
        scratch_pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnDeltaApplyTextdeltaStreamFunc = Option<
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
pub(super) type SvnDeltaChangeDirPropFunc = Option<
    unsafe extern "C" fn(
        dir_baton: *mut c_void,
        name: *const c_char,
        value: *const svn_string_t,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnDeltaChangeFilePropFunc = Option<
    unsafe extern "C" fn(
        file_baton: *mut c_void,
        name: *const c_char,
        value: *const svn_string_t,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnDeltaAbortEditFunc =
    Option<unsafe extern "C" fn(edit_baton: *mut c_void, pool: *mut AprPoolT) -> *mut svn_error_t>;

#[cfg(git_svn_rs_libsvn_linked)]
#[allow(dead_code)]
#[repr(C)]
pub(super) struct SvnDeltaEditorT {
    pub(super) set_target_revision: SvnDeltaSetTargetRevisionFunc,
    pub(super) open_root: SvnDeltaOpenRootFunc,
    pub(super) delete_entry: SvnDeltaDeleteEntryFunc,
    pub(super) add_directory: SvnDeltaAddDirectoryFunc,
    pub(super) open_directory: SvnDeltaOpenDirectoryFunc,
    pub(super) change_dir_prop: SvnDeltaChangeDirPropFunc,
    pub(super) close_directory: SvnDeltaCloseDirectoryFunc,
    pub(super) absent_directory: SvnDeltaAbsentDirectoryFunc,
    pub(super) add_file: SvnDeltaAddFileFunc,
    pub(super) open_file: SvnDeltaOpenFileFunc,
    pub(super) apply_textdelta: SvnDeltaApplyTextdeltaFunc,
    pub(super) change_file_prop: SvnDeltaChangeFilePropFunc,
    pub(super) close_file: SvnDeltaCloseFileFunc,
    pub(super) absent_file: SvnDeltaAbsentFileFunc,
    pub(super) close_edit: SvnDeltaCloseEditFunc,
    pub(super) abort_edit: SvnDeltaAbortEditFunc,
    pub(super) apply_textdelta_stream: SvnDeltaApplyTextdeltaStreamFunc,
}

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnRaReporterSetPathFunc = Option<
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
pub(super) type SvnRaReporterDeletePathFunc = Option<
    unsafe extern "C" fn(
        report_baton: *mut c_void,
        path: *const c_char,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnRaReporterLinkPathFunc = Option<
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
pub(super) type SvnRaReporterFinishReportFunc = Option<
    unsafe extern "C" fn(report_baton: *mut c_void, pool: *mut AprPoolT) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnRaReporterAbortReportFunc = Option<
    unsafe extern "C" fn(report_baton: *mut c_void, pool: *mut AprPoolT) -> *mut svn_error_t,
>;

#[cfg(git_svn_rs_libsvn_linked)]
#[allow(dead_code)]
#[repr(C)]
pub(super) struct SvnRaReporter3T {
    pub(super) set_path: SvnRaReporterSetPathFunc,
    pub(super) delete_path: SvnRaReporterDeletePathFunc,
    pub(super) link_path: SvnRaReporterLinkPathFunc,
    pub(super) finish_report: SvnRaReporterFinishReportFunc,
    pub(super) abort_report: SvnRaReporterAbortReportFunc,
}

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
pub(super) struct svn_log_changed_path2_t {
    pub(super) action: c_char,
    pub(super) copyfrom_path: *const c_char,
    pub(super) copyfrom_rev: c_long,
    pub(super) node_kind: c_int,
    pub(super) text_modified: c_int,
    pub(super) props_modified: c_int,
}

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
pub(super) struct svn_log_entry_t {
    pub(super) changed_paths: *mut AprHashT,
    pub(super) revision: c_long,
    pub(super) revprops: *mut AprHashT,
    pub(super) has_children: c_int,
    pub(super) changed_paths2: *mut AprHashT,
    pub(super) non_inheritable: c_int,
    pub(super) subtractive_merge: c_int,
}

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
pub(super) struct svn_dirent_t {
    pub(super) kind: c_int,
    pub(super) size: i64,
    pub(super) has_props: c_int,
    pub(super) created_rev: c_long,
    pub(super) time: i64,
    pub(super) last_author: *const c_char,
}

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
pub(super) struct apr_array_header_t {
    pub(super) pool: *mut AprPoolT,
    pub(super) elt_size: c_int,
    pub(super) nelts: c_int,
    pub(super) nalloc: c_int,
    pub(super) elts: *mut c_char,
}

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) enum AprPoolT {}

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) enum AprHashT {}

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) enum AprHashIndexT {}

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) enum SvnStreamT {}

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) enum SvnTxdeltaWindowT {}

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) enum SvnTxdeltaStreamT {}

#[cfg(git_svn_rs_libsvn_linked)]
#[repr(C)]
pub(super) struct SvnRaCallbacks2T {
    pub(super) open_tmp_file: *mut c_void,
    pub(super) auth_baton: *mut SvnAuthBatonT,
}

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) enum SvnAuthBatonT {}

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) enum SvnAuthProviderObjectT {}

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) enum SvnRaSessionT {}

#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type AprAbortFunc = Option<unsafe extern "C" fn(c_int) -> c_int>;
#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnAuthPlaintextPromptFunc = Option<
    unsafe extern "C" fn(*mut c_int, *const c_char, *mut c_void, *mut AprPoolT) -> *mut svn_error_t,
>;
#[cfg(git_svn_rs_libsvn_linked)]
pub(super) type SvnAuthSimplePromptFunc = Option<
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
unsafe extern "C" {
    pub(super) fn apr_initialize() -> c_int;
    pub(super) fn apr_pool_create_ex(
        newpool: *mut *mut AprPoolT,
        parent: *mut AprPoolT,
        abort_fn: AprAbortFunc,
        allocator: *mut c_void,
    ) -> c_int;
    pub(super) fn apr_pool_destroy(pool: *mut AprPoolT);
    pub(super) fn apr_palloc(pool: *mut AprPoolT, size: usize) -> *mut c_void;
    pub(super) fn apr_pstrdup(pool: *mut AprPoolT, string: *const c_char) -> *const c_char;
    pub(super) fn apr_array_make(
        pool: *mut AprPoolT,
        nelts: c_int,
        elt_size: c_int,
    ) -> *mut apr_array_header_t;
    pub(super) fn apr_array_push(array: *mut apr_array_header_t) -> *mut c_void;
    pub(super) fn apr_hash_get(
        hash: *mut AprHashT,
        key: *const c_void,
        key_len: isize,
    ) -> *mut c_void;
    pub(super) fn apr_hash_first(pool: *mut AprPoolT, hash: *mut AprHashT) -> *mut AprHashIndexT;
    pub(super) fn apr_hash_next(index: *mut AprHashIndexT) -> *mut AprHashIndexT;
    pub(super) fn apr_hash_this(
        index: *mut AprHashIndexT,
        key: *mut *const c_void,
        key_len: *mut isize,
        value: *mut *mut c_void,
    );
    pub(super) fn svn_stringbuf_create_empty(pool: *mut AprPoolT) -> *mut svn_stringbuf_t;
    #[allow(dead_code)]
    pub(super) fn svn_stringbuf_ncreate(
        bytes: *const c_char,
        size: usize,
        pool: *mut AprPoolT,
    ) -> *mut svn_stringbuf_t;
    pub(super) fn svn_stream_from_stringbuf(
        buffer: *mut svn_stringbuf_t,
        pool: *mut AprPoolT,
    ) -> *mut SvnStreamT;
    #[allow(dead_code)]
    pub(super) fn svn_stream_empty(pool: *mut AprPoolT) -> *mut SvnStreamT;
    #[allow(dead_code)]
    pub(super) fn svn_txdelta_apply(
        source: *mut SvnStreamT,
        target: *mut SvnStreamT,
        result_digest: *mut c_char,
        error_info: *const c_char,
        pool: *mut AprPoolT,
        handler: *mut SvnTxdeltaWindowHandlerFunc,
        handler_baton: *mut *mut c_void,
    );
    pub(super) fn svn_txdelta_next_window(
        window: *mut *mut SvnTxdeltaWindowT,
        stream: *mut SvnTxdeltaStreamT,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    pub(super) fn svn_subr_version() -> *const svn_version_t;
    pub(super) fn svn_err_best_message(
        error: *const svn_error_t,
        buffer: *mut c_char,
        buffer_size: usize,
    ) -> *const c_char;
    pub(super) fn svn_error_clear(error: *mut svn_error_t);
    pub(super) fn svn_error_create(
        apr_err: c_int,
        child: *mut svn_error_t,
        message: *const c_char,
    ) -> *mut svn_error_t;
    #[allow(dead_code)]
    pub(super) fn svn_delta_default_editor(pool: *mut AprPoolT) -> *mut SvnDeltaEditorT;
    #[allow(dead_code)]
    pub(super) fn svn_ra_do_update3(
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
    pub(super) fn svn_ra_do_switch3(
        session: *mut SvnRaSessionT,
        reporter: *mut *const SvnRaReporter3T,
        report_baton: *mut *mut c_void,
        revision_to_switch_to: c_long,
        switch_target: *const c_char,
        depth: c_int,
        switch_url: *const c_char,
        send_copyfrom_args: c_int,
        ignore_ancestry: c_int,
        switch_editor: *const SvnDeltaEditorT,
        switch_baton: *mut c_void,
        result_pool: *mut AprPoolT,
        scratch_pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    pub(super) fn svn_auth_get_simple_provider2(
        provider: *mut *mut SvnAuthProviderObjectT,
        plaintext_prompt_func: SvnAuthPlaintextPromptFunc,
        prompt_baton: *mut c_void,
        pool: *mut AprPoolT,
    );
    pub(super) fn svn_auth_get_simple_prompt_provider(
        provider: *mut *mut SvnAuthProviderObjectT,
        prompt_func: SvnAuthSimplePromptFunc,
        prompt_baton: *mut c_void,
        retry_limit: c_int,
        pool: *mut AprPoolT,
    );
    pub(super) fn svn_auth_get_username_provider(
        provider: *mut *mut SvnAuthProviderObjectT,
        pool: *mut AprPoolT,
    );
    pub(super) fn svn_auth_open(
        auth_baton: *mut *mut SvnAuthBatonT,
        providers: *const apr_array_header_t,
        pool: *mut AprPoolT,
    );
    pub(super) fn svn_auth_set_parameter(
        auth_baton: *mut SvnAuthBatonT,
        name: *const c_char,
        value: *const c_void,
    );
    pub(super) fn svn_config_get_config(
        config: *mut *mut AprHashT,
        config_dir: *const c_char,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    pub(super) fn svn_ra_initialize(pool: *mut AprPoolT) -> *mut svn_error_t;
    pub(super) fn svn_ra_create_callbacks(
        callbacks: *mut *mut SvnRaCallbacks2T,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    pub(super) fn svn_ra_open5(
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
    pub(super) fn svn_ra_get_latest_revnum(
        session: *mut SvnRaSessionT,
        latest_revnum: *mut c_long,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    pub(super) fn svn_ra_rev_proplist(
        session: *mut SvnRaSessionT,
        revision: c_long,
        props: *mut *mut AprHashT,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    pub(super) fn svn_ra_reparent(
        session: *mut SvnRaSessionT,
        url: *const c_char,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    pub(super) fn svn_ra_check_path(
        session: *mut SvnRaSessionT,
        path: *const c_char,
        revision: c_long,
        kind: *mut c_int,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    pub(super) fn svn_ra_get_uuid2(
        session: *mut SvnRaSessionT,
        uuid: *mut *const c_char,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    pub(super) fn svn_ra_get_repos_root2(
        session: *mut SvnRaSessionT,
        url: *mut *const c_char,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    pub(super) fn svn_ra_get_file(
        session: *mut SvnRaSessionT,
        path: *const c_char,
        revision: c_long,
        stream: *mut SvnStreamT,
        fetched_rev: *mut c_long,
        props: *mut *mut AprHashT,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    pub(super) fn svn_ra_get_dir2(
        session: *mut SvnRaSessionT,
        dirents: *mut *mut AprHashT,
        fetched_rev: *mut c_long,
        props: *mut *mut AprHashT,
        path: *const c_char,
        revision: c_long,
        dirent_fields: c_uint,
        pool: *mut AprPoolT,
    ) -> *mut svn_error_t;
    pub(super) fn svn_ra_get_log2(
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
