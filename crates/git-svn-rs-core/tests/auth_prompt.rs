#[cfg(unix)]
use git_svn_rs_core::svn::auth::AskpassAuthPrompt;
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

#[cfg(unix)]
#[test]
fn askpass_program_receives_prompt_and_trims_only_line_endings() {
    let temp = tempfile::tempdir().unwrap();
    let log = temp.path().join("prompt.log");
    let script = write_askpass_script(
        temp.path(),
        "askpass",
        &format!(
            "#!/bin/sh\nprintf '%s' \"$1\" > '{}'\nprintf ' secret with spaces \\r\\n'\n",
            log.display()
        ),
    );
    let creds = AskpassAuthPrompt::new(script)
        .simple(AuthRequest {
            realm: Some("svn://example/repo".to_string()),
            default_username: Some("alice".to_string()),
            may_save: true,
            no_auth_cache: true,
        })
        .unwrap();

    assert_eq!(creds.password, " secret with spaces ");
    assert!(!creds.may_save);
    assert_eq!(
        std::fs::read_to_string(log).unwrap(),
        "Password for 'alice@svn://example/repo': "
    );
}

#[cfg(unix)]
#[test]
fn askpass_requests_a_missing_username_before_the_password() {
    let temp = tempfile::tempdir().unwrap();
    let log = temp.path().join("prompts.log");
    let script = write_askpass_script(
        temp.path(),
        "askpass-credentials",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$1\" >> '{}'\ncase \"$1\" in\n  Username*) printf 'alice\\r\\n' ;;\n  *) printf 'secret\\n' ;;\nesac\n",
            log.display()
        ),
    );
    let creds = AskpassAuthPrompt::new(script)
        .simple(AuthRequest {
            realm: Some("svn://example/repo".to_string()),
            default_username: None,
            may_save: true,
            no_auth_cache: false,
        })
        .unwrap();

    assert_eq!(creds.username, "alice");
    assert_eq!(creds.password, "secret");
    assert_eq!(
        std::fs::read_to_string(log).unwrap(),
        "Username for 'svn://example/repo': \nPassword for 'alice@svn://example/repo': \n"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn askpass_retries_while_the_program_is_temporarily_open_for_writing() {
    let temp = tempfile::tempdir().unwrap();
    let script = write_askpass_script(
        temp.path(),
        "temporarily-busy-askpass",
        "#!/bin/sh\nprintf 'secret\\n'\n",
    );
    let writer = std::fs::OpenOptions::new()
        .write(true)
        .open(&script)
        .unwrap();
    let release_writer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(20));
        drop(writer);
    });

    let result = AskpassAuthPrompt::new(script).simple(auth_request());
    release_writer.join().unwrap();

    assert_eq!(result.unwrap().password, "secret");
}

#[cfg(unix)]
#[test]
fn askpass_failure_and_empty_answer_are_secret_safe() {
    let temp = tempfile::tempdir().unwrap();
    let failing = write_askpass_script(
        temp.path(),
        "failing",
        "#!/bin/sh\nprintf 'do-not-leak' >&2\nexit 9\n",
    );
    let error = AskpassAuthPrompt::new(failing)
        .simple(auth_request())
        .unwrap_err();
    assert!(error.contains("status"));
    assert!(error.contains('9'));
    assert!(!error.contains("do-not-leak"));

    let empty = write_askpass_script(temp.path(), "empty", "#!/bin/sh\nprintf '\\n'\n");
    assert!(
        AskpassAuthPrompt::new(empty)
            .simple(auth_request())
            .unwrap_err()
            .contains("empty password")
    );

    let empty_username =
        write_askpass_script(temp.path(), "empty-username", "#!/bin/sh\nprintf '\\n'\n");
    let error = AskpassAuthPrompt::new(empty_username)
        .simple(AuthRequest {
            default_username: None,
            ..auth_request()
        })
        .unwrap_err();
    assert!(error.contains("empty username"));
}

#[cfg(unix)]
fn auth_request() -> AuthRequest {
    AuthRequest {
        realm: Some("svn://example/repo".to_string()),
        default_username: Some("alice".to_string()),
        may_save: false,
        no_auth_cache: true,
    }
}

#[cfg(unix)]
fn write_askpass_script(
    directory: &std::path::Path,
    name: &str,
    contents: &str,
) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = directory.join(name);
    std::fs::write(&path, contents).unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).unwrap();
    path
}
