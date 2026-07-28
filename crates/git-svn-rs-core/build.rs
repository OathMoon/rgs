fn main() {
    println!("cargo:rerun-if-env-changed=PKG_CONFIG");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_LIBDIR");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_SYSROOT_DIR");
    println!("cargo:rerun-if-env-changed=VCPKG_ROOT");
    println!("cargo:rerun-if-env-changed=VCPKG_DEFAULT_TRIPLET");
    println!("cargo:rerun-if-env-changed=VCPKGRS_TRIPLET");
    println!("cargo:rerun-if-env-changed=VCPKGRS_DYNAMIC");
    println!("cargo:rustc-check-cfg=cfg(git_svn_rs_libsvn_linked)");

    if std::env::var_os("CARGO_FEATURE_SVN_LIBSVN").is_none() {
        return;
    }

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let result = if target_os == "windows" {
        probe_vcpkg()
    } else {
        probe_pkg_config()
    };

    match result {
        Ok(()) => println!("cargo:rustc-cfg=git_svn_rs_libsvn_linked"),
        Err(error) => {
            println!(
                "cargo:warning=svn-libsvn feature enabled but Subversion libraries were not found: {error}"
            );
            if target_os == "windows" {
                println!(
                    "cargo:warning=install with `vcpkg install subversion:x64-windows` and set VCPKG_ROOT/VCPKG_DEFAULT_TRIPLET or VCPKGRS_TRIPLET"
                );
            } else {
                println!(
                    "cargo:warning=install the system Subversion development package and pkg-config (for Ubuntu/Debian: `apt install libsvn-dev pkg-config`)"
                );
            }
        }
    }
}

fn probe_vcpkg() -> Result<(), String> {
    let mut config = vcpkg::Config::new();
    config.emit_includes(true);

    if std::env::var_os("VCPKGRS_TRIPLET").is_none() {
        let triplet =
            std::env::var("VCPKG_DEFAULT_TRIPLET").unwrap_or_else(|_| "x64-windows".to_string());
        config.target_triplet(triplet);
    }

    config
        .find_package("subversion")
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn probe_pkg_config() -> Result<(), String> {
    const SVN_LIBRARIES: [(&str, &str); 3] = [
        ("libsvn_ra", "svn_ra-1"),
        ("libsvn_delta", "svn_delta-1"),
        ("libsvn_subr", "svn_subr-1"),
    ];
    const APR_LIBRARIES: [(&str, &str); 2] = [("apr-1", "apr-1"), ("apr-util-1", "aprutil-1")];

    for (package, _) in SVN_LIBRARIES {
        run_pkg_config(&["--atleast-version=1.14", package])?;
    }

    for (package, library) in SVN_LIBRARIES.into_iter().chain(APR_LIBRARIES) {
        let libdir = run_pkg_config(&["--variable=libdir", package])?;
        println!("cargo:rustc-link-search=native={}", libdir.trim());
        println!("cargo:rustc-link-lib={library}");
    }

    Ok(())
}

fn run_pkg_config(args: &[&str]) -> Result<String, String> {
    let program = std::env::var_os("PKG_CONFIG").unwrap_or_else(|| "pkg-config".into());
    let output = std::process::Command::new(&program)
        .args(args)
        .output()
        .map_err(|error| format!("could not run {}: {error}", program.to_string_lossy()))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "{} {} failed with {}: {}",
            program.to_string_lossy(),
            args.join(" "),
            output.status,
            stderr.trim()
        ))
    }
}
