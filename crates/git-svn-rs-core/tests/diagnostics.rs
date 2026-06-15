use git_svn_rs_core::diagnostics;

#[test]
fn reports_libsvn_feature_state() {
    let expected = if cfg!(feature = "svn-libsvn") {
        "enabled"
    } else {
        "disabled"
    };

    assert_eq!(diagnostics::libsvn_feature_status(), expected);
}
