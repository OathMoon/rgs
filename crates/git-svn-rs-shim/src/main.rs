use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("git-svn shim failed: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<u8, String> {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let status = Command::new(git_svn_rs_binary())
        .args(args)
        .status()
        .map_err(|err| format!("could not launch git-svn-rs: {err}"))?;

    Ok(status.code().unwrap_or(1).try_into().unwrap_or(1))
}

fn git_svn_rs_binary() -> PathBuf {
    if let Some(dir) = env::current_exe()
        .ok()
        .and_then(|current_exe| current_exe.parent().map(PathBuf::from))
    {
        let sibling = dir.join(exe_name("git-svn-rs"));
        if sibling.is_file() {
            return sibling;
        }
    }

    PathBuf::from(exe_name("git-svn-rs"))
}

fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}
