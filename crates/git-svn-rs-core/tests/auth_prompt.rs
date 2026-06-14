use git_svn_rs_core::svn::auth::{AuthPrompt, AuthRequest, MockAuthPrompt};

#[test]
fn username_option_overrides_default_username() {
    let prompt = MockAuthPrompt::new()
        .with_username("alice")
        .with_password("secret");
    let creds = prompt
        .simple(AuthRequest {
            realm: Some("repo".to_string()),
            default_username: Some("bob".to_string()),
            may_save: true,
            no_auth_cache: true,
        })
        .unwrap();

    assert_eq!(creds.username, "alice");
    assert_eq!(creds.password, "secret");
    assert!(!creds.may_save);
}

#[test]
fn askpass_fallback_can_be_mocked_without_terminal_input() {
    let prompt = MockAuthPrompt::new().with_askpass_answer("askpass-secret");
    let creds = prompt
        .simple(AuthRequest {
            realm: Some("repo".to_string()),
            default_username: Some("alice".to_string()),
            may_save: true,
            no_auth_cache: false,
        })
        .unwrap();

    assert_eq!(creds.username, "alice");
    assert_eq!(creds.password, "askpass-secret");
}
