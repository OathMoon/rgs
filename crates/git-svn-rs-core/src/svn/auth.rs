use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct AuthRequest {
    pub realm: Option<String>,
    pub default_username: Option<String>,
    pub may_save: bool,
    pub no_auth_cache: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub username: String,
    pub password: String,
    pub may_save: bool,
}

pub trait AuthPrompt: Send + Sync {
    fn simple(&self, request: AuthRequest) -> Result<Credentials, String>;
}

#[derive(Debug, Clone)]
pub struct AskpassAuthPrompt {
    program: OsString,
}

impl AskpassAuthPrompt {
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
        }
    }

    pub fn from_environment() -> Option<Self> {
        Self::from_programs(
            std::env::var_os("GIT_ASKPASS"),
            std::env::var_os("SSH_ASKPASS"),
        )
    }

    fn from_programs(git_askpass: Option<OsString>, ssh_askpass: Option<OsString>) -> Option<Self> {
        git_askpass.or(ssh_askpass).map(Self::new)
    }
}

impl AuthPrompt for AskpassAuthPrompt {
    fn simple(&self, request: AuthRequest) -> Result<Credentials, String> {
        let username = request.default_username.unwrap_or_default();
        let realm = request.realm.unwrap_or_else(|| "SVN".to_string());
        let output = Command::new(&self.program)
            .arg(format!("Password for '{username}@{realm}': "))
            .output()
            .map_err(|error| format!("SVN askpass failed to start: {error}"))?;
        if !output.status.success() {
            return Err(format!("SVN askpass exited with status {}", output.status));
        }
        let password = String::from_utf8(output.stdout)
            .map_err(|_| "SVN askpass returned a non-UTF-8 password".to_string())?
            .trim_end_matches(['\r', '\n'])
            .to_string();
        if password.is_empty() {
            return Err("SVN askpass returned an empty password".to_string());
        }
        Ok(Credentials {
            username,
            password,
            may_save: request.may_save && !request.no_auth_cache,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TerminalAuthPrompt;

impl AuthPrompt for TerminalAuthPrompt {
    fn simple(&self, request: AuthRequest) -> Result<Credentials, String> {
        let username = request.default_username.unwrap_or_default();
        let realm = request.realm.unwrap_or_else(|| "SVN".to_string());
        let password = read_terminal_password(&format!("Password for '{username}@{realm}': "))?;
        if password.is_empty() {
            return Err("SVN terminal prompt returned an empty password".to_string());
        }
        Ok(Credentials {
            username,
            password,
            may_save: request.may_save && !request.no_auth_cache,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthOperation {
    Read,
    Write,
}

pub fn prompted_password(
    realm: &str,
    username: Option<&str>,
    config_dir: Option<&str>,
    no_auth_cache: bool,
    operation: AuthOperation,
) -> Result<Option<String>, String> {
    let Some(username) = username else {
        return Ok(None);
    };
    let askpass = AskpassAuthPrompt::from_environment();
    if let Some(askpass) = askpass.as_ref() {
        return prompted_password_with(realm, username, no_auth_cache, Some(askpass), None);
    }
    if !terminal_prompt_enabled(
        std::io::stdin().is_terminal() && std::io::stderr().is_terminal(),
        std::env::var_os("GIT_TERMINAL_PROMPT").as_deref(),
    ) {
        return Ok(None);
    }
    if !no_auth_cache {
        let probe = probe_svn_read_access(realm, username, config_dir);
        match probe {
            SvnReadProbe::Accessible if operation == AuthOperation::Read => return Ok(None),
            SvnReadProbe::Indeterminate => return Ok(None),
            SvnReadProbe::Accessible | SvnReadProbe::AuthenticationRequired => {}
        }
    }
    let terminal = TerminalAuthPrompt;
    prompted_password_with(realm, username, no_auth_cache, None, Some(&terminal))
}

fn prompted_password_with(
    realm: &str,
    username: &str,
    no_auth_cache: bool,
    askpass: Option<&dyn AuthPrompt>,
    terminal: Option<&dyn AuthPrompt>,
) -> Result<Option<String>, String> {
    let Some(prompt) = askpass.or(terminal) else {
        return Ok(None);
    };
    prompt
        .simple(AuthRequest {
            realm: Some(realm.to_string()),
            default_username: Some(username.to_string()),
            may_save: !no_auth_cache,
            no_auth_cache,
        })
        .map(|credentials| Some(credentials.password))
}

fn terminal_prompt_enabled(is_terminal: bool, setting: Option<&std::ffi::OsStr>) -> bool {
    if !is_terminal {
        return false;
    }
    setting.is_none_or(|value| {
        !matches!(
            value.to_string_lossy().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        )
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SvnReadProbe {
    Accessible,
    AuthenticationRequired,
    Indeterminate,
}

fn probe_svn_read_access(realm: &str, username: &str, config_dir: Option<&str>) -> SvnReadProbe {
    let mut command = Command::new("svn");
    command.env("LC_ALL", "C").arg("--non-interactive");
    if let Some(config_dir) = config_dir {
        command.args(["--config-dir", config_dir]);
    }
    let target = crate::svn::target_without_peg_revision(realm);
    let output = command
        .args([
            "--username",
            username,
            "info",
            "--show-item",
            "repos-root-url",
            &target,
        ])
        .output();
    match output {
        Ok(output) if output.status.success() => SvnReadProbe::Accessible,
        Ok(output) if is_svn_authentication_error(&output.stderr) => {
            SvnReadProbe::AuthenticationRequired
        }
        Ok(_) | Err(_) => SvnReadProbe::Indeterminate,
    }
}

fn is_svn_authentication_error(stderr: &[u8]) -> bool {
    let stderr = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    [
        "e170001",
        "e215004",
        "authentication failed",
        "authorization failed",
        "could not authenticate",
        "can't get username or password",
    ]
    .iter()
    .any(|needle| stderr.contains(needle))
}

#[cfg(unix)]
fn read_terminal_password(prompt: &str) -> Result<String, String> {
    use std::os::fd::AsRawFd;

    let stdin = std::io::stdin();
    let fd = stdin.as_raw_fd();
    let mut original = std::mem::MaybeUninit::<libc::termios>::uninit();
    // SAFETY: fd is the live stdin descriptor and `original` points to writable termios storage.
    if unsafe { libc::tcgetattr(fd, original.as_mut_ptr()) } != 0 {
        return Err(format!(
            "failed to read terminal settings for SVN password: {}",
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: tcgetattr initialized `original` after the successful return above.
    let original = unsafe { original.assume_init() };
    let mut hidden = original;
    hidden.c_lflag &= !libc::ECHO;
    // SAFETY: fd remains live and `hidden` is a valid termios value derived from tcgetattr.
    if unsafe { libc::tcsetattr(fd, libc::TCSAFLUSH, &hidden) } != 0 {
        return Err(format!(
            "failed to disable terminal echo for SVN password: {}",
            std::io::Error::last_os_error()
        ));
    }
    let guard = UnixEchoGuard { fd, original };
    write_terminal_prompt(prompt)?;
    let mut password = String::new();
    let read = stdin.read_line(&mut password);
    drop(guard);
    finish_terminal_prompt()?;
    read.map_err(|error| format!("failed to read SVN password from terminal: {error}"))?;
    Ok(password.trim_end_matches(['\r', '\n']).to_string())
}

#[cfg(unix)]
struct UnixEchoGuard {
    fd: std::os::fd::RawFd,
    original: libc::termios,
}

#[cfg(unix)]
impl Drop for UnixEchoGuard {
    fn drop(&mut self) {
        // SAFETY: fd is stdin for this short-lived guard and original came from tcgetattr.
        unsafe {
            libc::tcsetattr(self.fd, libc::TCSANOW, &self.original);
        }
    }
}

#[cfg(windows)]
fn read_terminal_password(prompt: &str) -> Result<String, String> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        ENABLE_ECHO_INPUT, GetConsoleMode, GetStdHandle, STD_INPUT_HANDLE, SetConsoleMode,
    };

    // SAFETY: GetStdHandle has no pointer arguments and returns the process stdin handle.
    let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        return Err("failed to acquire terminal input for SVN password".to_string());
    }
    let mut original = 0;
    // SAFETY: handle is checked above and original is valid writable storage.
    if unsafe { GetConsoleMode(handle, &mut original) } == 0 {
        return Err(format!(
            "failed to read terminal settings for SVN password: {}",
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: handle is a console input handle and the mode only clears echo input.
    if unsafe { SetConsoleMode(handle, original & !ENABLE_ECHO_INPUT) } == 0 {
        return Err(format!(
            "failed to disable terminal echo for SVN password: {}",
            std::io::Error::last_os_error()
        ));
    }
    let guard = WindowsEchoGuard { handle, original };
    write_terminal_prompt(prompt)?;
    let mut password = String::new();
    let read = std::io::stdin().read_line(&mut password);
    drop(guard);
    finish_terminal_prompt()?;
    read.map_err(|error| format!("failed to read SVN password from terminal: {error}"))?;
    Ok(password.trim_end_matches(['\r', '\n']).to_string())
}

#[cfg(windows)]
struct WindowsEchoGuard {
    handle: windows_sys::Win32::Foundation::HANDLE,
    original: u32,
}

#[cfg(windows)]
impl Drop for WindowsEchoGuard {
    fn drop(&mut self) {
        // SAFETY: handle and original were returned by GetConsoleMode for this guard.
        unsafe {
            windows_sys::Win32::System::Console::SetConsoleMode(self.handle, self.original);
        }
    }
}

#[cfg(not(any(unix, windows)))]
fn read_terminal_password(_prompt: &str) -> Result<String, String> {
    Err("SVN terminal password input is unsupported on this platform".to_string())
}

fn write_terminal_prompt(prompt: &str) -> Result<(), String> {
    let mut stderr = std::io::stderr();
    write!(stderr, "{prompt}")
        .and_then(|()| stderr.flush())
        .map_err(|error| format!("failed to write SVN terminal prompt: {error}"))
}

fn finish_terminal_prompt() -> Result<(), String> {
    writeln!(std::io::stderr())
        .map_err(|error| format!("failed to finish SVN terminal prompt: {error}"))
}

#[derive(Debug, Default, Clone)]
pub struct MockAuthPrompt {
    username: Option<String>,
    password: Option<String>,
    askpass_answer: Option<String>,
}

impl MockAuthPrompt {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_username(mut self, username: &str) -> Self {
        self.username = Some(username.to_string());
        self
    }

    pub fn with_password(mut self, password: &str) -> Self {
        self.password = Some(password.to_string());
        self
    }

    pub fn with_askpass_answer(mut self, answer: &str) -> Self {
        self.askpass_answer = Some(answer.to_string());
        self
    }
}

impl AuthPrompt for MockAuthPrompt {
    fn simple(&self, request: AuthRequest) -> Result<Credentials, String> {
        Ok(Credentials {
            username: self
                .username
                .clone()
                .or(request.default_username)
                .unwrap_or_default(),
            password: self
                .password
                .clone()
                .or_else(|| self.askpass_answer.clone())
                .unwrap_or_default(),
            may_save: request.may_save && !request.no_auth_cache,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AskpassAuthPrompt, MockAuthPrompt, prompted_password_with, terminal_prompt_enabled,
    };
    use std::ffi::{OsStr, OsString};

    #[test]
    fn git_askpass_takes_precedence_over_ssh_askpass() {
        let prompt = AskpassAuthPrompt::from_programs(
            Some(OsString::from("git-askpass")),
            Some(OsString::from("ssh-askpass")),
        )
        .unwrap();
        assert_eq!(prompt.program, OsString::from("git-askpass"));

        let prompt =
            AskpassAuthPrompt::from_programs(None, Some(OsString::from("ssh-askpass"))).unwrap();
        assert_eq!(prompt.program, OsString::from("ssh-askpass"));
    }

    #[test]
    fn askpass_precedes_terminal_and_terminal_is_the_final_fallback() {
        let askpass = MockAuthPrompt::new().with_askpass_answer("askpass-secret");
        let terminal = MockAuthPrompt::new().with_password("terminal-secret");
        assert_eq!(
            prompted_password_with("svn://repo", "alice", true, Some(&askpass), Some(&terminal),)
                .unwrap()
                .as_deref(),
            Some("askpass-secret")
        );
        assert_eq!(
            prompted_password_with("svn://repo", "alice", true, None, Some(&terminal),)
                .unwrap()
                .as_deref(),
            Some("terminal-secret")
        );
        assert_eq!(
            prompted_password_with("svn://repo", "alice", true, None, None).unwrap(),
            None
        );
    }

    #[test]
    fn terminal_prompt_requires_a_tty_and_honors_git_disable_values() {
        assert!(!terminal_prompt_enabled(false, None));
        assert!(terminal_prompt_enabled(true, None));
        assert!(terminal_prompt_enabled(true, Some(OsStr::new("1"))));
        for value in ["0", "false", "NO", "Off"] {
            assert!(!terminal_prompt_enabled(true, Some(OsStr::new(value))));
        }
    }

    #[test]
    fn recognizes_svn_authentication_failures_without_matching_other_errors() {
        for message in [
            "svn: E170001: Can't get username or password",
            "svn: E215004: Authentication failed and interactive prompting is disabled",
            "Authorization failed",
        ] {
            assert!(super::is_svn_authentication_error(message.as_bytes()));
        }
        assert!(!super::is_svn_authentication_error(
            b"svn: E170013: Unable to connect to a repository"
        ));
    }
}
