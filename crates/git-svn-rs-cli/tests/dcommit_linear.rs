use assert_cmd::Command;
use git_svn_rs_core::dcommit::journal::{
    BatchState, DcommitJournal, DcommitTargetIdentity, EntryState, JournalEntry, JournalStore,
};
use predicates::prelude::*;

#[allow(dead_code)]
#[path = "../../git-svn-rs-core/tests/support/svn_fixture.rs"]
mod svn_fixture;

use svn_fixture::{
    StandardSvnFixture, SvnServe, SvnToolPolicy, require_svn_tools, require_svnserve,
};

#[test]
fn dcommit_dry_run_lists_local_commits_after_tracked_svn_ref() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    make_commit(&work, "local-one.txt", "one\n", "local one");
    make_commit(&work, "local-two.txt", "two\n", "local two");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Dcommit dry-run"))
        .stdout(predicate::str::contains("mock://repo/trunk"))
        .stdout(predicate::str::contains("refs/remotes/git-svn"))
        .stdout(predicate::str::contains("r2"))
        .stdout(predicate::str::contains(
            "Would commit 2 local Git commit(s)",
        ))
        .stdout(predicate::str::contains("local one"))
        .stdout(predicate::str::contains("local two"));
}

#[test]
fn dcommit_dry_run_reports_no_local_commits() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No local commits to dcommit."));
}

#[test]
fn dcommit_mock_write_back_registers_linear_commit() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    make_commit(&work, "local.txt", "local\n", "local change");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Committed 1 local Git commit(s)"))
        .stdout(predicate::str::contains("r3"));

    let head = git_stdout(&work, &["rev-parse", "HEAD"]);
    let tracked = git_stdout(&work, &["rev-parse", "refs/remotes/git-svn"]);
    assert_eq!(tracked, head);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["find-rev", "HEAD"])
        .assert()
        .success()
        .stdout("3\n");
}

#[test]
fn dcommit_mock_executes_typed_rename_and_copy_plan() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    std::fs::copy(work.join("src/lib.rs"), work.join("src/copied.rs")).unwrap();
    run_git(&work, &["mv", "src/lib.rs", "src/moved.rs"]);
    run_git(&work, &["add", "src/copied.rs"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "rename and copy through typed plan",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("r3"));

    assert_eq!(
        git_stdout(&work, &["rev-parse", "refs/remotes/git-svn"]),
        git_stdout(&work, &["rev-parse", "HEAD"])
    );
}

#[test]
fn dcommit_fails_closed_before_mock_write_back_for_unfinished_journal() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    let tracked_before = git_stdout(&work, &["rev-parse", "refs/remotes/git-svn"]);
    make_commit(&work, "local.txt", "local\n", "local change");
    let head = git_stdout(&work, &["rev-parse", "HEAD"]);
    let (rev_map_path, rev_map_before) = mock_rev_map_snapshot(&work);
    write_mock_dcommit_journal(
        &work,
        &tracked_before,
        &head,
        BatchState::Submitting,
        EntryState::Ready {
            expected_base_revision: 2,
            expected_tracking_oid: tracked_before.clone(),
        },
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unfinished dcommit journal found"));

    assert_eq!(
        git_stdout(&work, &["rev-parse", "refs/remotes/git-svn"]),
        tracked_before
    );
    assert_eq!(std::fs::read(rev_map_path).unwrap(), rev_map_before);
}

#[test]
fn dcommit_rejects_local_commit_recorded_by_completed_ledger() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    let tracked_before = git_stdout(&work, &["rev-parse", "refs/remotes/git-svn"]);
    make_commit(&work, "local.txt", "local\n", "already submitted");
    let head = git_stdout(&work, &["rev-parse", "HEAD"]);
    let (rev_map_path, rev_map_before) = mock_rev_map_snapshot(&work);
    write_mock_dcommit_journal(
        &work,
        &tracked_before,
        &head,
        BatchState::Complete,
        EntryState::FetchedVerified {
            svn_revision: 3,
            imported_oid: head.clone(),
        },
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "local commits overlap completed dcommit ledger",
        ));

    assert_eq!(
        git_stdout(&work, &["rev-parse", "refs/remotes/git-svn"]),
        tracked_before
    );
    assert_eq!(std::fs::read(rev_map_path).unwrap(), rev_map_before);
}

#[test]
fn dcommit_writes_linear_commit_to_file_svn_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(work.join("src/lib.rs"), "pub fn answer() -> u8 { 43 }\n").unwrap();
    run_git(&work, &["add", "src/lib.rs"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "change answer",
            "-m",
            "full dcommit message body",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Committed 1 local Git commit(s)"))
        .stdout(predicate::str::contains("change answer"));

    let output = std::process::Command::new("svn")
        .args(["log", "-l", "1", &fixture.url()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "svn log failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let log = String::from_utf8_lossy(&output.stdout);
    assert!(log.contains("change answer"));
    assert!(log.contains("full dcommit message body"));
}

#[test]
fn dcommit_rejects_dirty_work_tree_before_file_svn_write() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );
    std::fs::write(work.join("src/lib.rs"), "pub fn answer() -> u8 { 50 }\n").unwrap();
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-am",
            "dirty preflight",
        ],
    );
    std::fs::write(work.join("untracked.txt"), "must block dcommit\n").unwrap();
    let before = svn_stdout(&["info", "--show-item", "revision", &fixture.url()]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "dcommit requires a clean index and working tree",
        ));

    assert_eq!(
        svn_stdout(&["info", "--show-item", "revision", &fixture.url()]),
        before
    );
}

#[test]
fn dcommit_resumes_post_fetch_failure_without_duplicate_file_svn_commit() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");
    let authors_prog = recovery_authors_prog(temp.path(), false);
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "clone",
            &fixture.url(),
            "work",
            "--stdlayout",
            "--authors-prog",
            authors_prog.to_str().unwrap(),
        ])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );
    std::fs::write(work.join("src/lib.rs"), "pub fn answer() -> u8 { 51 }\n").unwrap();
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-am",
            "recover post fetch",
        ],
    );
    recovery_authors_prog(temp.path(), true);
    let before = svn_stdout(&["info", "--show-item", "revision", &fixture.url()])
        .trim()
        .parse::<u32>()
        .unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("post-submit"));
    let submitted = svn_stdout(&["info", "--show-item", "revision", &fixture.url()])
        .trim()
        .parse::<u32>()
        .unwrap();
    assert_eq!(submitted, before + 1);

    recovery_authors_prog(temp.path(), false);
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("recover post fetch"));
    assert_eq!(
        svn_stdout(&["info", "--show-item", "revision", &fixture.url()])
            .trim()
            .parse::<u32>()
            .unwrap(),
        submitted,
        "resuming a Submitted journal must not create another SVN revision"
    );
    assert_eq!(
        git_stdout(&work, &["show", "refs/remotes/origin/trunk:src/lib.rs"]),
        "pub fn answer() -> u8 { 51 }"
    );
}

#[test]
fn dcommit_revision_option_does_not_limit_post_commit_fetch_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(work.join("src/lib.rs"), "pub fn answer() -> u8 { 44 }\n").unwrap();
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-am",
            "revision-limited dcommit answer",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase", "--revision", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("revision-limited dcommit answer"));

    assert_eq!(
        git_stdout(&work, &["show", "refs/remotes/origin/trunk:src/lib.rs"]),
        "pub fn answer() -> u8 { 44 }"
    );
}

#[test]
fn dcommit_writes_linear_commit_to_svnserve_when_tools_exist() {
    match require_svn_tools().and_then(|()| require_svnserve()) {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let fixture = StandardSvnFixture::create().unwrap();
    fixture.allow_anonymous_write().unwrap();
    let server = SvnServe::start(fixture.root()).unwrap();
    let parent = tempfile::tempdir().unwrap();
    let work = parent.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(parent.path())
        .args(["clone", &server.repo_url(), "work", "--stdlayout"])
        .assert()
        .success();

    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );
    std::fs::write(work.join("src/lib.rs"), "pub fn answer() -> u8 { 47 }\n").unwrap();
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-am",
            "remote answer",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("remote answer"));

    assert_eq!(
        svn_stdout(&["cat", &format!("{}/trunk/src/lib.rs", server.repo_url())]),
        "pub fn answer() -> u8 { 47 }\n"
    );
}

#[test]
fn dcommit_writes_to_authenticated_svnserve_with_password_when_tools_exist() {
    match require_svn_tools().and_then(|()| require_svnserve()) {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let fixture = StandardSvnFixture::create().unwrap();
    fixture.require_write_auth("alice", "secret").unwrap();
    let server = SvnServe::start(fixture.root()).unwrap();
    let parent = tempfile::tempdir().unwrap();
    let work = parent.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(parent.path())
        .args(["clone", &server.repo_url(), "work", "--stdlayout"])
        .assert()
        .success();

    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );
    std::fs::write(work.join("src/lib.rs"), "pub fn answer() -> u8 { 48 }\n").unwrap();
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-am",
            "authenticated remote answer",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args([
            "dcommit",
            "--no-rebase",
            "--username",
            "alice",
            "--password",
            "secret",
            "--no-auth-cache",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("authenticated remote answer"));

    assert_eq!(
        svn_stdout(&["cat", &format!("{}/trunk/src/lib.rs", server.repo_url())]),
        "pub fn answer() -> u8 { 48 }\n"
    );
}

#[test]
fn dcommit_fetches_after_authenticated_svnserve_write_when_reads_require_auth() {
    match require_svn_tools().and_then(|()| require_svnserve()) {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let fixture = StandardSvnFixture::create().unwrap();
    fixture.require_read_write_auth("alice", "secret").unwrap();
    let server = SvnServe::start(fixture.root()).unwrap();
    let parent = tempfile::tempdir().unwrap();
    let work = parent.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(parent.path())
        .args([
            "clone",
            &server.repo_url(),
            "work",
            "--stdlayout",
            "--username",
            "alice",
            "--password",
            "secret",
            "--no-auth-cache",
        ])
        .assert()
        .success();

    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );
    std::fs::write(work.join("src/lib.rs"), "pub fn answer() -> u8 { 49 }\n").unwrap();
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-am",
            "authenticated readback answer",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args([
            "dcommit",
            "--no-rebase",
            "--username",
            "alice",
            "--password",
            "secret",
            "--no-auth-cache",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("authenticated readback answer"));

    assert_eq!(
        git_stdout(&work, &["show", "refs/remotes/origin/trunk:src/lib.rs"]),
        "pub fn answer() -> u8 { 49 }"
    );
}

#[test]
fn dcommit_writes_to_explicit_file_svn_commit_url_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(work.join("src/lib.rs"), "pub fn answer() -> u8 { 99 }\n").unwrap();
    run_git(&work, &["add", "src/lib.rs"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "commit to branch url",
        ],
    );

    let branch_url = format!("{}/branches/main", fixture.url());
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase", "--commit-url", &branch_url])
        .assert()
        .success()
        .stdout(predicate::str::contains("commit to branch url"));

    assert_eq!(
        svn_stdout(&[
            "cat",
            &format!("{}/branches/main/src/lib.rs", fixture.url())
        ]),
        "pub fn answer() -> u8 { 99 }\n"
    );
    assert_eq!(
        svn_stdout(&["cat", &format!("{}/trunk/src/lib.rs", fixture.url())]),
        "pub fn answer() -> u8 { 42 }\n"
    );
}

#[test]
fn dcommit_writes_explicit_mergeinfo_to_file_svn_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(work.join("src/lib.rs"), "pub fn answer() -> u8 { 44 }\n").unwrap();
    run_git(&work, &["add", "src/lib.rs"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "write explicit mergeinfo",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args([
            "dcommit",
            "--no-rebase",
            "--mergeinfo",
            "/branches/main:1-2",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("write explicit mergeinfo"));

    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:mergeinfo",
            &format!("{}/trunk", fixture.url())
        ]),
        "/branches/main:1-2"
    );
}

#[test]
fn dcommit_writes_eol_style_from_gitattributes_to_file_svn_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(work.join(".gitattributes"), "*.txt svn:eol-style=LF\n").unwrap();
    std::fs::write(work.join("notes.txt"), "one\ntwo\n").unwrap();
    run_git(&work, &["add", ".gitattributes", "notes.txt"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "add eol attributed text",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("add eol attributed text"));

    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:eol-style",
            &format!("{}/trunk/notes.txt", fixture.url())
        ]),
        "LF"
    );
}

#[test]
fn dcommit_honors_svn_config_auto_props_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");
    let svn_config = temp.path().join("svn-config");
    std::fs::create_dir_all(&svn_config).unwrap();
    std::fs::write(
        svn_config.join("config"),
        "[miscellany]\nenable-auto-props = yes\n[auto-props]\n*.auto = svn:eol-style=LF\n",
    )
    .unwrap();
    let svn_config = svn_config.to_string_lossy().to_string();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );
    run_git(&work, &["config", "svn-remote.svn.config-dir", &svn_config]);

    std::fs::write(work.join("generated.auto"), "one\ntwo\n").unwrap();
    run_git(&work, &["add", "generated.auto"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "add auto-props text",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("add auto-props text"));

    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:eol-style",
            &format!("{}/trunk/generated.auto", fixture.url())
        ]),
        "LF"
    );
}

#[test]
fn dcommit_honors_command_line_svn_config_auto_props_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");
    let svn_config = temp.path().join("svn-config");
    std::fs::create_dir_all(&svn_config).unwrap();
    std::fs::write(
        svn_config.join("config"),
        "[miscellany]\nenable-auto-props = yes\n[auto-props]\n*.cmdauto = svn:eol-style=LF\n",
    )
    .unwrap();
    let svn_config = svn_config.to_string_lossy().to_string();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(work.join("generated.cmdauto"), "one\ntwo\n").unwrap();
    run_git(&work, &["add", "generated.cmdauto"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "add command line auto-props text",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase", "--config-dir", &svn_config])
        .assert()
        .success()
        .stdout(predicate::str::contains("add command line auto-props text"));

    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:eol-style",
            &format!("{}/trunk/generated.cmdauto", fixture.url())
        ]),
        "LF"
    );
}

#[test]
fn dcommit_filters_invalid_svn_properties_from_gitattributes_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(
        work.join(".gitattributes"),
        "*.txt svn-properties=svn:eol-style=LF;svn:keywords=Id;;missing;=ignored;svn:mime-type=; svn:eol-style=CRLF\n",
    )
    .unwrap();
    std::fs::write(work.join("notes.txt"), "one\ntwo\n").unwrap();
    run_git(&work, &["add", ".gitattributes", "notes.txt"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "add svn-properties attributed text",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "add svn-properties attributed text",
        ));

    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:eol-style",
            &format!("{}/trunk/notes.txt", fixture.url())
        ]),
        "CRLF"
    );
    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:keywords",
            &format!("{}/trunk/notes.txt", fixture.url())
        ]),
        "Id"
    );
}

#[test]
fn dcommit_uses_final_svn_properties_attribute_state_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(
        work.join(".gitattributes"),
        "*.txt svn-properties=svn:eol-style=LF;svn:keywords=Id\nreplacement.txt svn-properties=svn:eol-style=CRLF\nunset.txt -svn-properties svn:eol-style=CRLF\nunspecified.txt !svn-properties svn:eol-style=CRLF\n",
    )
    .unwrap();
    for path in ["replacement.txt", "unset.txt", "unspecified.txt"] {
        std::fs::write(work.join(path), "one\ntwo\n").unwrap();
    }
    run_git(
        &work,
        &[
            "add",
            ".gitattributes",
            "replacement.txt",
            "unset.txt",
            "unspecified.txt",
        ],
    );
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "apply final svn-properties state",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("apply final svn-properties state"));

    for path in ["replacement.txt", "unset.txt", "unspecified.txt"] {
        let url = format!("{}/trunk/{path}", fixture.url());
        assert_eq!(
            svn_stdout(&["propget", "--strict", "svn:eol-style", &url]),
            "CRLF"
        );
        svn_propget_missing("svn:keywords", &url);
    }
}

#[test]
fn dcommit_writes_mime_type_from_gitattributes_to_file_svn_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(
        work.join(".gitattributes"),
        "*.json svn:mime-type=application/json\n",
    )
    .unwrap();
    std::fs::write(work.join("payload.json"), "{\"ok\":true}\n").unwrap();
    run_git(&work, &["add", ".gitattributes", "payload.json"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "add mime attributed json",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("add mime attributed json"));

    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:mime-type",
            &format!("{}/trunk/payload.json", fixture.url())
        ]),
        "application/json"
    );
}

#[test]
fn dcommit_writes_keywords_from_gitattributes_to_file_svn_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(work.join(".gitattributes"), "*.rs svn:keywords=Id\n").unwrap();
    std::fs::write(work.join("version.rs"), "pub const ID: &str = \"$Id$\";\n").unwrap();
    run_git(&work, &["add", ".gitattributes", "version.rs"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "add keyword attributed rust file",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("add keyword attributed rust file"));

    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:keywords",
            &format!("{}/trunk/version.rs", fixture.url())
        ]),
        "Id"
    );
}

#[test]
fn dcommit_writes_needs_lock_from_gitattributes_to_file_svn_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(work.join(".gitattributes"), "*.lock svn:needs-lock=x\n").unwrap();
    std::fs::write(work.join("manual.lock"), "locked\n").unwrap();
    run_git(&work, &["add", ".gitattributes", "manual.lock"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "add needs-lock attributed file",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("add needs-lock attributed file"));

    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:needs-lock",
            &format!("{}/trunk/manual.lock", fixture.url())
        ]),
        "*"
    );
}

#[test]
fn dcommit_writes_boolean_needs_lock_from_gitattributes_to_file_svn_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(work.join(".gitattributes"), "*.lock svn:needs-lock\n").unwrap();
    std::fs::write(work.join("boolean.lock"), "locked\n").unwrap();
    run_git(&work, &["add", ".gitattributes", "boolean.lock"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "add boolean needs-lock attributed file",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "add boolean needs-lock attributed file",
        ));

    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:needs-lock",
            &format!("{}/trunk/boolean.lock", fixture.url())
        ]),
        "*"
    );
}

#[test]
fn dcommit_writes_boolean_executable_from_gitattributes_to_file_svn_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(work.join(".gitattributes"), "*.cmd svn:executable\n").unwrap();
    std::fs::write(work.join("manual.cmd"), "echo manual\n").unwrap();
    run_git(&work, &["add", ".gitattributes", "manual.cmd"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "add executable attributed command",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "add executable attributed command",
        ));

    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:executable",
            &format!("{}/trunk/manual.cmd", fixture.url())
        ]),
        "*"
    );
}

#[test]
fn dcommit_writes_valued_executable_from_gitattributes_to_file_svn_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(work.join(".gitattributes"), "*.cmd svn:executable=x\n").unwrap();
    std::fs::write(work.join("valued.cmd"), "echo valued\n").unwrap();
    run_git(&work, &["add", ".gitattributes", "valued.cmd"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "add valued executable attributed command",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "add valued executable attributed command",
        ));

    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:executable",
            &format!("{}/trunk/valued.cmd", fixture.url())
        ]),
        "*"
    );
}

#[test]
fn dcommit_writes_boolean_special_from_gitattributes_to_file_svn_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(work.join(".gitattributes"), "*.manual-link svn:special\n").unwrap();
    std::fs::write(work.join("manual.manual-link"), "link src/lib.rs").unwrap();
    run_git(&work, &["add", ".gitattributes", "manual.manual-link"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "add special attributed manual link",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "add special attributed manual link",
        ));

    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:special",
            &format!("{}/trunk/manual.manual-link", fixture.url())
        ]),
        "*"
    );
    assert_eq!(
        svn_stdout(&[
            "cat",
            &format!("{}/trunk/manual.manual-link", fixture.url())
        ]),
        "link src/lib.rs"
    );
}

#[test]
fn dcommit_writes_valued_special_from_gitattributes_to_file_svn_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(work.join(".gitattributes"), "*.valued-link svn:special=x\n").unwrap();
    std::fs::write(work.join("manual.valued-link"), "link src/lib.rs").unwrap();
    run_git(&work, &["add", ".gitattributes", "manual.valued-link"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "add valued special attributed manual link",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "add valued special attributed manual link",
        ));

    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:special",
            &format!("{}/trunk/manual.valued-link", fixture.url())
        ]),
        "*"
    );
    assert_eq!(
        svn_stdout(&[
            "cat",
            &format!("{}/trunk/manual.valued-link", fixture.url())
        ]),
        "link src/lib.rs"
    );
}

#[test]
fn dcommit_direct_gitattributes_property_can_be_cleared_by_later_rule_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(
        work.join(".gitattributes"),
        "*.lock svn:needs-lock\nclear.lock -svn:needs-lock\n",
    )
    .unwrap();
    std::fs::write(work.join("keep.lock"), "locked\n").unwrap();
    std::fs::write(work.join("clear.lock"), "unlocked\n").unwrap();
    run_git(&work, &["add", ".gitattributes", "keep.lock", "clear.lock"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "clear direct needs-lock attribute",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "clear direct needs-lock attribute",
        ));

    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:needs-lock",
            &format!("{}/trunk/keep.lock", fixture.url())
        ]),
        "*"
    );
    svn_propget_missing(
        "svn:needs-lock",
        &format!("{}/trunk/clear.lock", fixture.url()),
    );
}

#[test]
fn dcommit_gitattributes_later_matching_rule_overrides_earlier_rule_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(
        work.join(".gitattributes"),
        "*.txt svn:eol-style=LF\nnotes.txt svn:eol-style=CRLF\n",
    )
    .unwrap();
    std::fs::write(work.join("notes.txt"), "one\ntwo\n").unwrap();
    run_git(&work, &["add", ".gitattributes", "notes.txt"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "add overridden eol attributed text",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "add overridden eol attributed text",
        ));

    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:eol-style",
            &format!("{}/trunk/notes.txt", fixture.url())
        ]),
        "CRLF"
    );
}

#[test]
fn dcommit_gitattributes_directory_wildcard_matches_file_svn_path_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::create_dir_all(work.join("docs")).unwrap();
    std::fs::write(work.join(".gitattributes"), "docs/*.txt svn:eol-style=LF\n").unwrap();
    std::fs::write(work.join("docs/notes.txt"), "one\ntwo\n").unwrap();
    run_git(&work, &["add", ".gitattributes", "docs/notes.txt"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "add directory wildcard attributed text",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "add directory wildcard attributed text",
        ));

    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:eol-style",
            &format!("{}/trunk/docs/notes.txt", fixture.url())
        ]),
        "LF"
    );
}

#[test]
fn dcommit_rebases_after_file_svn_write_by_default() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    make_commit(&work, "rebased.txt", "rebased\n", "add rebased file");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("dcommit")
        .assert()
        .success()
        .stdout(predicate::str::contains("Committed 1 local Git commit(s)"));

    assert_eq!(
        git_stdout(&work, &["rev-parse", "HEAD"]),
        git_stdout(&work, &["rev-parse", "refs/remotes/origin/trunk"])
    );
}

#[test]
fn dcommit_writes_file_adds_and_deletes_to_file_svn_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(work.join("added.txt"), "added\n").unwrap();
    std::fs::remove_file(work.join("run.sh")).unwrap();
    run_git(&work, &["add", "added.txt", "run.sh"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "add and delete files",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("add and delete files"));

    let listing = svn_stdout(&["list", "-R", &format!("{}/trunk", fixture.url())]);
    assert!(listing.lines().any(|line| line == "added.txt"));
    assert!(!listing.lines().any(|line| line == "run.sh"));
}

#[test]
fn dcommit_writes_file_rename_to_file_svn_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    run_git(&work, &["mv", "src/lib.rs", "src/main.rs"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "rename library file",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rename library file"));

    let listing = svn_stdout(&["list", "-R", &format!("{}/trunk", fixture.url())]);
    assert!(listing.lines().any(|line| line == "src/main.rs"));
    assert!(!listing.lines().any(|line| line == "src/lib.rs"));

    let log = svn_stdout(&["log", "--xml", "-v", "-l", "1", &fixture.url()]);
    assert!(log.contains("copyfrom-path=\"/trunk/src/lib.rs\""));
    assert!(log.contains(">/trunk/src/main.rs<"));
}

#[test]
fn dcommit_writes_file_rename_to_new_directory_to_file_svn_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::create_dir_all(work.join("moved")).unwrap();
    run_git(&work, &["mv", "src/lib.rs", "moved/main.rs"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "rename library file into directory",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rename library file into directory",
        ));

    let listing = svn_stdout(&["list", "-R", &format!("{}/trunk", fixture.url())]);
    assert!(listing.lines().any(|line| line == "moved/main.rs"));
    assert!(!listing.lines().any(|line| line == "src/lib.rs"));

    let log = svn_stdout(&["log", "--xml", "-v", "-l", "1", &fixture.url()]);
    assert!(log.contains("copyfrom-path=\"/trunk/src/lib.rs\""));
    assert!(log.contains(">/trunk/moved/main.rs<"));
}

#[test]
fn dcommit_writes_file_copy_to_file_svn_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::copy(work.join("src/lib.rs"), work.join("src/lib-copy.rs")).unwrap();
    run_git(&work, &["add", "src/lib-copy.rs"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "copy library file",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("copy library file"));

    let listing = svn_stdout(&["list", "-R", &format!("{}/trunk", fixture.url())]);
    assert!(listing.lines().any(|line| line == "src/lib.rs"));
    assert!(listing.lines().any(|line| line == "src/lib-copy.rs"));

    let log = svn_stdout(&["log", "--xml", "-v", "-l", "1", &fixture.url()]);
    assert!(log.contains("copyfrom-path=\"/trunk/src/lib.rs\""));
    assert!(log.contains(">/trunk/src/lib-copy.rs<"));
}

#[test]
fn dcommit_writes_executable_property_to_file_svn_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    std::fs::write(work.join("tool.sh"), "#!/bin/sh\necho tool\n").unwrap();
    run_git(&work, &["add", "tool.sh"]);
    run_git(&work, &["update-index", "--chmod=+x", "tool.sh"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "add executable tool",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("add executable tool"));

    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:executable",
            &format!("{}/trunk/tool.sh", fixture.url())
        ]),
        "*"
    );
}

#[test]
fn dcommit_removes_executable_property_from_file_svn_when_mode_is_cleared() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    run_git(&work, &["update-index", "--chmod=-x", "run.sh"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "clear executable bit",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("clear executable bit"));

    svn_propget_missing("svn:executable", &format!("{}/trunk/run.sh", fixture.url()));
}

#[test]
fn dcommit_writes_special_property_to_file_svn_when_tools_exist() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    let blob = git_stdout_with_stdin(&work, &["hash-object", "-w", "--stdin"], "src/lib.rs");
    run_git(
        &work,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            "120000",
            &blob,
            "new-link",
        ],
    );
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "add special link",
        ],
    );
    run_git(&work, &["reset", "--hard", "HEAD"]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("add special link"));

    assert_eq!(
        svn_stdout(&[
            "propget",
            "--strict",
            "svn:special",
            &format!("{}/trunk/new-link", fixture.url())
        ]),
        "*"
    );
    assert_eq!(
        svn_stdout(&["cat", &format!("{}/trunk/new-link", fixture.url())]),
        "link src/lib.rs"
    );
}

#[test]
fn dcommit_removes_special_property_from_file_svn_when_link_becomes_file() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    let blob = git_stdout_with_stdin(&work, &["hash-object", "-w", "--stdin"], "regular\n");
    run_git(
        &work,
        &[
            "update-index",
            "--cacheinfo",
            "100644",
            &blob,
            "link-to-lib",
        ],
    );
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "turn link into file",
        ],
    );
    run_git(&work, &["reset", "--hard", "HEAD"]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("turn link into file"));

    svn_propget_missing(
        "svn:special",
        &format!("{}/trunk/link-to-lib", fixture.url()),
    );
    assert_eq!(
        svn_stdout(&["cat", &format!("{}/trunk/link-to-lib", fixture.url())]),
        "regular\n"
    );
}

#[test]
fn dcommit_mergeinfo_reports_v1_scope_message() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--dry-run", "--mergeinfo", "/branches/main:1-2"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "explicit mergeinfo accepted for dry-run: /branches/main:1-2",
        ))
        .stdout(predicate::str::contains(
            "automatic mergeinfo generation is not implemented in v1",
        ));
}

#[test]
fn dcommit_without_dry_run_is_guarded_for_non_mock_urls() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    make_commit(&work, "local.txt", "local\n", "local change");
    run_git(
        &work,
        &["config", "svn-remote.svn.url", "https://svn.example/trunk"],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("dcommit")
        .assert()
        .failure()
        .stderr(predicate::str::contains("mock://"))
        .stderr(predicate::str::contains("not implemented"));
}

fn clone_mock_repo(parent: &std::path::Path) -> std::path::PathBuf {
    let work = parent.join("work");
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(parent)
        .args(["clone", "mock://repo/trunk", "work"])
        .assert()
        .success();
    work
}

fn make_commit(work: &std::path::Path, path: &str, content: &str, message: &str) {
    std::fs::write(work.join(path), content).unwrap();
    run_git(work, &["add", path]);
    run_git(
        work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            message,
        ],
    );
}

fn mock_rev_map_snapshot(work: &std::path::Path) -> (std::path::PathBuf, Vec<u8>) {
    let metadata = work.join(".git/svn/git-svn");
    let rev_map_path = std::fs::read_dir(&metadata)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".rev_map.")
        })
        .expect("mock clone must create its rev_map in the mapping metadata directory");
    let bytes = std::fs::read(&rev_map_path).unwrap();
    (rev_map_path, bytes)
}

fn write_mock_dcommit_journal(
    work: &std::path::Path,
    tracked_oid: &str,
    head_oid: &str,
    batch_state: BatchState,
    entry_state: EntryState,
) {
    let metadata = work.join(".git/svn/git-svn");
    let store = JournalStore::new(metadata.join("dcommit-journal"));
    let lock = store.acquire_lock().unwrap();
    store
        .save(
            &lock,
            &DcommitJournal {
                target: DcommitTargetIdentity {
                    remote_id: "svn".to_owned(),
                    repository_root_url: "mock://repo/trunk".to_owned(),
                    repository_uuid: "mock-uuid".to_owned(),
                    mapping_ref: "refs/remotes/git-svn".to_owned(),
                    rev_map_path: metadata
                        .join(".rev_map.mock-uuid")
                        .to_string_lossy()
                        .into_owned(),
                    commit_url: "mock://repo/trunk".to_owned(),
                },
                original_base_revision: 2,
                original_base_oid: tracked_oid.to_owned(),
                original_head: head_oid.to_owned(),
                no_rebase: true,
                config_fingerprint: "1010".to_owned(),
                entries: vec![JournalEntry {
                    git_oid: head_oid.to_owned(),
                    base_oid: tracked_oid.to_owned(),
                    plan_fingerprint: "2020".to_owned(),
                    message_fingerprint: "3030".to_owned(),
                    state: entry_state,
                }],
                batch_state,
            },
        )
        .unwrap();
}

fn run_git(work: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .current_dir(work)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout(work: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .current_dir(work)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn git_stdout_with_stdin(work: &std::path::Path, args: &[&str], stdin: &str) -> String {
    use std::io::Write;

    let mut child = std::process::Command::new("git")
        .current_dir(work)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn recovery_authors_prog(dir: &std::path::Path, fail: bool) -> std::path::PathBuf {
    #[cfg(windows)]
    {
        let path = dir.join("recovery-authors-prog.cmd");
        let script = if fail {
            "@echo off\r\nexit /b 1\r\n"
        } else {
            "@echo off\r\necho Recovery Author ^<recovery@example.com^>\r\n"
        };
        std::fs::write(&path, script).unwrap();
        path
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;

        let path = dir.join("recovery-authors-prog.sh");
        let script = if fail {
            "#!/bin/sh\nexit 1\n"
        } else {
            "#!/bin/sh\necho 'Recovery Author <recovery@example.com>'\n"
        };
        std::fs::write(&path, script).unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
        path
    }
}

fn svn_stdout(args: &[&str]) -> String {
    let output = std::process::Command::new("svn")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "svn {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn svn_propget_missing(name: &str, url: &str) {
    let output = std::process::Command::new("svn")
        .args(["propget", "--strict", name, url])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "svn propget unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}
