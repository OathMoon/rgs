use git_svn_rs_core::diagnostics;

#[test]
fn reports_package_baseline_and_platform_identity() {
    assert_eq!(diagnostics::package_version(), env!("CARGO_PKG_VERSION"));
    assert_eq!(diagnostics::FROZEN_GIT_SVN_VERSION, "2.54.0");
    assert_eq!(
        diagnostics::FROZEN_GIT_COMMIT,
        "0b13e48a3a30cdfa94e8ef842e24d6045ab3d015"
    );
    assert_eq!(
        diagnostics::platform(),
        format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH)
    );
}

#[test]
fn reports_libsvn_feature_state() {
    let expected = if cfg!(feature = "svn-libsvn") {
        "enabled"
    } else {
        "disabled"
    };

    assert_eq!(diagnostics::libsvn_feature_status(), expected);
}

#[test]
fn reports_libsvn_link_state() {
    let expected = if cfg!(feature = "svn-libsvn") {
        if cfg!(git_svn_rs_libsvn_linked) {
            "linked"
        } else {
            "not-linked"
        }
    } else {
        "not-compiled"
    };

    assert_eq!(diagnostics::libsvn_link_status(), expected);
}
