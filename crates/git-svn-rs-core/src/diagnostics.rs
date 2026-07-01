pub fn libsvn_feature_status() -> &'static str {
    if cfg!(feature = "svn-libsvn") {
        "enabled"
    } else {
        "disabled"
    }
}

pub fn libsvn_link_status() -> &'static str {
    #[cfg(feature = "svn-libsvn")]
    {
        match crate::svn::libsvn::LibSvnBackend::availability().link_status {
            crate::svn::libsvn::LibSvnLinkStatus::Linked => "linked",
            crate::svn::libsvn::LibSvnLinkStatus::NotLinked => "not-linked",
        }
    }

    #[cfg(not(feature = "svn-libsvn"))]
    {
        "not-compiled"
    }
}
