pub fn libsvn_feature_status() -> &'static str {
    if cfg!(feature = "svn-libsvn") {
        "enabled"
    } else {
        "disabled"
    }
}
