use clap::Parser;
use git_svn_rs_core::cli::{Cli, Command};

#[test]
fn parses_rebase_uppercase_merge_alias_and_strategy() {
    let cli = Cli::parse_from(["git-svn-rs", "rebase", "-M", "-s", "ort"]);
    let Command::Rebase(args) = cli.command else {
        panic!("expected rebase");
    };
    assert!(args.merge);
    assert_eq!(args.strategy.as_deref(), Some("ort"));
}

#[test]
fn parses_clone_with_standard_layout() {
    let cli = Cli::parse_from([
        "git-svn-rs",
        "clone",
        "file:///tmp/repo",
        "work",
        "--stdlayout",
        "--authors-file",
        "authors.txt",
    ]);

    match cli.command {
        Command::Clone(args) => {
            assert_eq!(args.url, "file:///tmp/repo");
            assert_eq!(args.path.as_deref(), Some("work"));
            assert!(args.layout.stdlayout);
            assert_eq!(args.shared.authors_file.as_deref(), Some("authors.txt"));
        }
        other => panic!("expected clone, got {other:?}"),
    }
}

#[test]
fn parses_dcommit_dry_run_commit_url() {
    let cli = Cli::parse_from([
        "git-svn-rs",
        "dcommit",
        "--dry-run",
        "--commit-url",
        "https://svn.example/write",
    ]);

    match cli.command {
        Command::Dcommit(args) => {
            assert!(args.dry_run);
            assert_eq!(
                args.commit_url.as_deref(),
                Some("https://svn.example/write")
            );
        }
        other => panic!("expected dcommit, got {other:?}"),
    }
}

#[test]
fn parses_dcommit_manual_revision_adoption() {
    let cli = Cli::try_parse_from(["git-svn-rs", "dcommit", "--adopt-revision", "42"]).unwrap();
    let Command::Dcommit(args) = cli.command else {
        panic!("expected dcommit");
    };
    assert_eq!(args.adopt_revision, Some(42));
    assert!(!args.dry_run);
}

#[test]
fn parses_dcommit_explicit_mergeinfo() {
    let cli = Cli::parse_from([
        "git-svn-rs",
        "dcommit",
        "--mergeinfo",
        "/branches/foo:1-10",
        "--dry-run",
    ]);

    match cli.command {
        Command::Dcommit(args) => {
            assert!(args.dry_run);
            assert_eq!(args.mergeinfo.as_deref(), Some("/branches/foo:1-10"));
        }
        other => panic!("expected dcommit, got {other:?}"),
    }
}

#[test]
fn parses_dcommit_auth_options() {
    let cli = Cli::parse_from([
        "git-svn-rs",
        "dcommit",
        "--username",
        "alice",
        "--password",
        "secret",
        "--no-auth-cache",
    ]);

    match cli.command {
        Command::Dcommit(args) => {
            assert_eq!(args.shared.username.as_deref(), Some("alice"));
            assert_eq!(args.shared.password.as_deref(), Some("secret"));
            assert!(args.shared.no_auth_cache);
        }
        other => panic!("expected dcommit, got {other:?}"),
    }
}

#[test]
fn parses_known_unsupported_command() {
    let cli = Cli::parse_from(["git-svn-rs", "branch", "feature"]);
    assert!(matches!(cli.command, Command::Branch(_)));
}

#[test]
fn parses_log_git_log_passthrough_args() {
    let cli = Cli::parse_from(["git-svn-rs", "log", "--oneline", "--", "src/lib.rs"]);

    match cli.command {
        Command::Log(args) => {
            assert!(args.oneline);
            assert_eq!(args.git_log_args, ["src/lib.rs"]);
        }
        other => panic!("expected log, got {other:?}"),
    }
}

#[test]
fn parses_log_authors_file() {
    let cli = Cli::parse_from(["git-svn-rs", "log", "-A", "authors.txt"]);

    match cli.command {
        Command::Log(args) => {
            assert_eq!(args.authors_file.as_deref(), Some("authors.txt"));
        }
        other => panic!("expected log, got {other:?}"),
    }
}

#[test]
fn parses_log_non_recursive() {
    let cli = Cli::parse_from(["git-svn-rs", "log", "--non-recursive"]);

    let Command::Log(args) = cli.command else {
        panic!("expected log");
    };
    assert!(args.non_recursive);
}

#[test]
fn find_rev_before_and_after_conflict() {
    let err =
        Cli::try_parse_from(["git-svn-rs", "find-rev", "--before", "--after", "r3"]).unwrap_err();

    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}
