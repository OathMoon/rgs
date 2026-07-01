fn main() {
    println!("cargo:rerun-if-env-changed=VCPKG_ROOT");
    println!("cargo:rerun-if-env-changed=VCPKG_DEFAULT_TRIPLET");
    println!("cargo:rerun-if-env-changed=VCPKGRS_TRIPLET");
    println!("cargo:rerun-if-env-changed=VCPKGRS_DYNAMIC");
    println!("cargo:rustc-check-cfg=cfg(git_svn_rs_libsvn_linked)");

    if std::env::var_os("CARGO_FEATURE_SVN_LIBSVN").is_none() {
        return;
    }

    let mut config = vcpkg::Config::new();
    config.emit_includes(true);

    if std::env::var_os("VCPKGRS_TRIPLET").is_none() {
        let triplet =
            std::env::var("VCPKG_DEFAULT_TRIPLET").unwrap_or_else(|_| "x64-windows".to_string());
        config.target_triplet(triplet);
    }

    match config.find_package("subversion") {
        Ok(_) => {
            println!("cargo:rustc-cfg=git_svn_rs_libsvn_linked");
        }
        Err(error) => {
            println!(
                "cargo:warning=svn-libsvn feature enabled but vcpkg could not find subversion: {error}"
            );
            println!(
                "cargo:warning=install with `vcpkg install subversion:x64-windows` and set VCPKG_ROOT/VCPKG_DEFAULT_TRIPLET or VCPKGRS_TRIPLET"
            );
        }
    }
}
