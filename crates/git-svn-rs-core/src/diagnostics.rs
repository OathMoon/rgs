pub const FROZEN_GIT_SVN_VERSION: &str = "2.54.0";
pub const FROZEN_GIT_COMMIT: &str = "0b13e48a3a30cdfa94e8ef842e24d6045ab3d015";

pub fn package_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn platform() -> String {
    format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH)
}

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
