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
