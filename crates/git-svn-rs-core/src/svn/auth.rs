use std::ffi::OsString;
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

pub fn askpass_password(
    realm: &str,
    username: Option<&str>,
    no_auth_cache: bool,
) -> Result<Option<String>, String> {
    let Some(username) = username else {
        return Ok(None);
    };
    let Some(prompt) = AskpassAuthPrompt::from_environment() else {
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
    use super::AskpassAuthPrompt;
    use std::ffi::OsString;

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
}
