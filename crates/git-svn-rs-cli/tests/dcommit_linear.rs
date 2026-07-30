use assert_cmd::Command;
use git_svn_rs_core::dcommit::journal::{
    BatchState, DcommitJournal, DcommitTargetIdentity, EntryState, JournalEntry, JournalStore,
};
use git_svn_rs_core::rev_map::{ObjectFormat, RevMap};
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
fn dcommit_mock_rejects_commit_url_before_journal_or_write() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    let tracked_before = git_stdout(&work, &["rev-parse", "refs/remotes/git-svn"]);
    make_commit(&work, "local.txt", "local\n", "local change");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args([
            "dcommit",
            "--commit-url",
            "mock://repo/trunk",
            "--no-rebase",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--commit-url is not supported for mock://",
        ));

    assert_eq!(
        git_stdout(&work, &["rev-parse", "refs/remotes/git-svn"]),
        tracked_before
    );
    assert!(
        !work
            .join(".git/svn/refs/remotes/git-svn/dcommit-journal")
            .exists()
    );
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
fn dcommit_post_fetch_uses_the_resolved_named_remote_when_tools_exist() {
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
        &[
            "config",
            "--rename-section",
            "svn-remote.svn",
            "svn-remote.other",
        ],
    );
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );
    std::fs::write(work.join("named-remote.txt"), "named\n").unwrap();
    run_git(&work, &["add", "named-remote.txt"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "write through named remote",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Committed 1 local Git commit(s)"));

    assert_eq!(
        svn_stdout(&["cat", &format!("{}/trunk/named-remote.txt", fixture.url())]),
        "named\n"
    );
    assert_eq!(
        git_stdout(
            &work,
            &["show", "refs/remotes/origin/trunk:named-remote.txt"]
        ),
        "named"
    );
}

#[test]
fn dcommit_writes_peg_sensitive_url_and_file_targets_when_tools_exist() {
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
    fixture.create_peg_sensitive_trunk().unwrap();
    let url = format!("{}/trunk%40main", fixture.url());
    let work = temp.path().join("work");
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &url, "work"])
        .assert()
        .success();
    run_git(&work, &["checkout", "-b", "topic", "refs/remotes/git-svn"]);

    std::fs::write(work.join("note@draft.txt"), "peg-sensitive\n").unwrap();
    run_git(&work, &["add", "note@draft.txt"]);
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "add peg-sensitive file",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Committed 1 local Git commit(s)"));

    assert_eq!(
        svn_stdout(&[
            "cat",
            &format!("{}/trunk%40main/note%40draft.txt@", fixture.url())
        ]),
        "peg-sensitive\n"
    );
    assert_eq!(
        git_stdout(&work, &["show", "refs/remotes/git-svn:note@draft.txt"]),
        "peg-sensitive"
    );
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
    make_commit(
        &work,
        "queued-after-recovery.txt",
        "queued\n",
        "recover queued commit",
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
    let journal_directory = find_dcommit_journal(&work);
    let store = JournalStore::new(&journal_directory);
    let interrupted = store.load().unwrap().expect("interrupted dcommit journal");
    assert_eq!(interrupted.entries.len(), 2);
    assert!(matches!(
        interrupted.entries[0].state,
        EntryState::Submitted { svn_revision } if svn_revision == u64::from(submitted)
    ));
    assert!(matches!(interrupted.entries[1].state, EntryState::Queued));
    let first_log = svn_stdout(&["log", "--xml", &fixture.url()]);
    assert_eq!(
        first_log.matches("<msg>recover post fetch</msg>").count(),
        1
    );
    assert_eq!(
        first_log
            .matches("<msg>recover queued commit</msg>")
            .count(),
        0
    );

    recovery_authors_prog(temp.path(), false);
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("recover post fetch"))
        .stdout(predicate::str::contains("recover queued commit"));
    let completed_revision = svn_stdout(&["info", "--show-item", "revision", &fixture.url()])
        .trim()
        .parse::<u32>()
        .unwrap();
    assert_eq!(
        completed_revision,
        before + 2,
        "resume must fetch the first submission and submit only the queued commit"
    );
    let final_log = svn_stdout(&["log", "--xml", &fixture.url()]);
    assert_eq!(
        final_log.matches("<msg>recover post fetch</msg>").count(),
        1
    );
    assert_eq!(
        final_log
            .matches("<msg>recover queued commit</msg>")
            .count(),
        1
    );
    assert_eq!(
        git_stdout(&work, &["show", "refs/remotes/origin/trunk:src/lib.rs"]),
        "pub fn answer() -> u8 { 51 }"
    );
    assert_eq!(
        git_stdout(
            &work,
            &[
                "show",
                "refs/remotes/origin/trunk:queued-after-recovery.txt"
            ]
        ),
        "queued"
    );
    let completed = store.load().unwrap().expect("completed dcommit journal");
    assert_eq!(completed.batch_state, BatchState::Complete);
    assert_eq!(completed.entries.len(), 2);
    let rev_map_path = std::path::PathBuf::from(&completed.target.rev_map_path);
    let rev_map_path = if rev_map_path.is_absolute() {
        rev_map_path
    } else {
        work.join(rev_map_path)
    };
    let rev_map = RevMap::open_existing(rev_map_path, ObjectFormat::Sha1).unwrap();
    for entry in &completed.entries {
        let EntryState::FetchedVerified {
            svn_revision,
            imported_oid,
        } = &entry.state
        else {
            panic!("recovered queue entry was not verified: {:?}", entry.state);
        };
        assert_eq!(
            rev_map
                .get(u32::try_from(*svn_revision).unwrap())
                .unwrap()
                .as_deref(),
            Some(imported_oid.as_str())
        );
    }
    let EntryState::FetchedVerified { imported_oid, .. } = &completed.entries[1].state else {
        unreachable!()
    };
    assert_eq!(
        git_stdout(&work, &["rev-parse", "refs/remotes/origin/trunk"]),
        *imported_oid
    );
}

#[test]
fn dcommit_adopts_verified_in_flight_file_svn_revision_without_resubmitting() {
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
    std::fs::write(work.join("src/lib.rs"), "pub fn answer() -> u8 { 61 }\n").unwrap();
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-am",
            "adopt verified revision",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success();
    let submitted = svn_stdout(&["info", "--show-item", "revision", &fixture.url()])
        .trim()
        .parse::<u64>()
        .unwrap();

    let journal_directory = find_dcommit_journal(&work);
    let store = JournalStore::new(journal_directory);
    let lock = store.acquire_lock().unwrap();
    let mut journal = store.load().unwrap().expect("completed dcommit journal");
    journal.batch_state = BatchState::Submitting;
    journal.entries[0].state = EntryState::SubmissionInFlight {
        expected_base_revision: journal.original_base_revision,
        expected_tracking_oid: journal.original_base_oid.clone(),
    };
    store.save(&lock, &journal).unwrap();
    drop(lock);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args([
            "dcommit",
            "--no-rebase",
            "--adopt-revision",
            &submitted.to_string(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Committed 1 local Git commit(s)"));
    assert_eq!(
        svn_stdout(&["info", "--show-item", "revision", &fixture.url()])
            .trim()
            .parse::<u64>()
            .unwrap(),
        submitted,
        "manual adoption must not submit another SVN revision"
    );
    let recovered = store.load().unwrap().expect("reconciled dcommit journal");
    assert_eq!(recovered.batch_state, BatchState::Complete);
    assert!(matches!(
        recovered.entries[0].state,
        EntryState::FetchedVerified {
            svn_revision,
            ..
        } if svn_revision == submitted
    ));
}

#[cfg(unix)]
#[test]
fn dcommit_repeats_fetch_only_after_fetched_verified_save_failure() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    use std::os::unix::fs::PermissionsExt;

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
    make_commit(
        &work,
        "post-fetch-save.txt",
        "verified before save\n",
        "recover fetched verified save",
    );
    let local_head = git_stdout(&work, &["rev-parse", "HEAD"]);
    let journal_directory = work.join(".git/svn/refs/remotes/origin/trunk/dcommit-journal");
    assert!(!journal_directory.to_string_lossy().contains('"'));
    std::fs::write(
        &authors_prog,
        format!(
            "#!/bin/sh\nchmod 0555 \"{}\"\necho 'Recovery Author <recovery@example.com>'\n",
            journal_directory.display()
        ),
    )
    .unwrap();
    let before = fixture.latest_revision();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "could not persist dcommit journal",
        ));
    let submitted = fixture.latest_revision();
    assert_eq!(submitted, before + 1);

    let mut journal_permissions = std::fs::metadata(&journal_directory).unwrap().permissions();
    journal_permissions.set_mode(0o755);
    std::fs::set_permissions(&journal_directory, journal_permissions).unwrap();
    let store = JournalStore::new(&journal_directory);
    let interrupted = store.load().unwrap().expect("submitted dcommit journal");
    assert!(matches!(
        interrupted.entries[0].state,
        EntryState::Submitted { svn_revision } if svn_revision == u64::from(submitted)
    ));
    let tracking_oid = git_stdout(&work, &["rev-parse", "refs/remotes/origin/trunk"]);
    assert_ne!(tracking_oid, interrupted.original_base_oid);
    assert_eq!(git_stdout(&work, &["rev-parse", "HEAD"]), local_head);
    let rev_map_path = std::path::PathBuf::from(&interrupted.target.rev_map_path);
    let rev_map_path = if rev_map_path.is_absolute() {
        rev_map_path
    } else {
        work.join(rev_map_path)
    };
    assert_eq!(
        RevMap::open_existing(rev_map_path, ObjectFormat::Sha1)
            .unwrap()
            .get(submitted)
            .unwrap()
            .as_deref(),
        Some(tracking_oid.as_str())
    );
    assert_eq!(
        git_stdout(
            &work,
            &["show", "refs/remotes/origin/trunk:post-fetch-save.txt"]
        ),
        "verified before save"
    );
    let footer = git_stdout(
        &work,
        &["show", "-s", "--format=%B", "refs/remotes/origin/trunk"],
    );
    assert!(footer.contains(&format!("git-svn-id: {}/trunk@{submitted} ", fixture.url())));

    recovery_authors_prog(temp.path(), false);
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("recover fetched verified save"));
    assert_eq!(
        fixture.latest_revision(),
        submitted,
        "resume must repeat fetch/verification without resubmitting"
    );
    let completed = store.load().unwrap().expect("completed dcommit journal");
    assert_eq!(completed.batch_state, BatchState::Complete);
    assert!(matches!(
        &completed.entries[0].state,
        EntryState::FetchedVerified {
            svn_revision,
            imported_oid,
        } if *svn_revision == u64::from(submitted) && imported_oid == &tracking_oid
    ));
    let log = svn_stdout(&["log", "--xml", &fixture.url()]);
    assert_eq!(
        log.matches("<msg>recover fetched verified save</msg>")
            .count(),
        1
    );
}

#[cfg(unix)]
#[test]
fn dcommit_adopts_revision_after_submitted_journal_save_failure() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    use std::os::unix::fs::PermissionsExt;

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
    std::fs::write(work.join("src/lib.rs"), "pub fn answer() -> u8 { 62 }\n").unwrap();
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-am",
            "recover failed submitted save",
        ],
    );

    let journal_directory = work.join(".git/svn/refs/remotes/origin/trunk/dcommit-journal");
    assert!(!journal_directory.to_string_lossy().contains('"'));
    let hook = fixture.root().join("repo/hooks/post-commit");
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\nchmod 0555 \"{}\"\n",
            journal_directory.display()
        ),
    )
    .unwrap();
    let mut hook_permissions = std::fs::metadata(&hook).unwrap().permissions();
    hook_permissions.set_mode(0o755);
    std::fs::set_permissions(&hook, hook_permissions).unwrap();
    let before = fixture.latest_revision();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("durable outcome is ambiguous"))
        .stderr(predicate::str::contains(
            "submitted state could not be persisted",
        ));
    let submitted = fixture.latest_revision();
    assert_eq!(submitted, before + 1);

    let mut journal_permissions = std::fs::metadata(&journal_directory).unwrap().permissions();
    journal_permissions.set_mode(0o755);
    std::fs::set_permissions(&journal_directory, journal_permissions).unwrap();
    let store = JournalStore::new(&journal_directory);
    let journal = store.load().unwrap().expect("in-flight journal");
    assert!(matches!(
        journal.entries[0].state,
        EntryState::SubmissionInFlight { .. }
    ));

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("outcome is ambiguous"));
    assert_eq!(
        fixture.latest_revision(),
        submitted,
        "automatic retry of the in-flight journal must not resubmit"
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args([
            "dcommit",
            "--no-rebase",
            "--adopt-revision",
            &submitted.to_string(),
        ])
        .assert()
        .success();
    assert_eq!(
        fixture.latest_revision(),
        submitted,
        "manual adoption must not submit another SVN revision"
    );
    let recovered = store.load().unwrap().expect("reconciled dcommit journal");
    assert_eq!(recovered.batch_state, BatchState::Complete);
    assert!(matches!(
        recovered.entries[0].state,
        EntryState::FetchedVerified {
            svn_revision,
            ..
        } if svn_revision == u64::from(submitted)
    ));
}

#[cfg(unix)]
#[test]
fn dcommit_adopts_commit_url_revision_after_svn_commit_success_response_is_lost() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    use std::os::unix::fs::PermissionsExt;

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
    std::fs::write(work.join("src/lib.rs"), "pub fn answer() -> u8 { 63 }\n").unwrap();
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-am",
            "recover lost submit response",
        ],
    );

    let original_path = std::env::var_os("PATH").expect("PATH for svn fixture");
    let real_svn = std::env::split_paths(&original_path)
        .map(|directory| directory.join("svn"))
        .find(|candidate| candidate.is_file())
        .expect("real svn executable on PATH");
    let wrapper_directory = temp.path().join("svn-wrapper");
    std::fs::create_dir(&wrapper_directory).unwrap();
    let wrapper = wrapper_directory.join("svn");
    let marker = wrapper_directory.join("commit-response-lost");
    assert!(!real_svn.to_string_lossy().contains('"'));
    assert!(!marker.to_string_lossy().contains('"'));
    std::fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nis_commit=0\nfor arg in \"$@\"; do\n  if [ \"$arg\" = commit ]; then is_commit=1; fi\ndone\nif [ \"$is_commit\" = 1 ] && [ ! -e \"{}\" ]; then\n  : > \"{}\"\n  \"{}\" \"$@\" || exit $?\n  exit 1\nfi\nexec \"{}\" \"$@\"\n",
            marker.display(),
            marker.display(),
            real_svn.display(),
            real_svn.display()
        ),
    )
    .unwrap();
    let mut wrapper_permissions = std::fs::metadata(&wrapper).unwrap().permissions();
    wrapper_permissions.set_mode(0o755);
    std::fs::set_permissions(&wrapper, wrapper_permissions).unwrap();
    let wrapped_path = std::env::join_paths(
        std::iter::once(wrapper_directory.clone()).chain(std::env::split_paths(&original_path)),
    )
    .unwrap();
    let before = fixture.latest_revision();
    let branch_url = format!("{}/branches/main", fixture.url());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .env("PATH", &wrapped_path)
        .args(["dcommit", "--no-rebase", "--commit-url", &branch_url])
        .assert()
        .failure()
        .stderr(predicate::str::contains("outcome is ambiguous"));
    let submitted = fixture.latest_revision();
    assert_eq!(submitted, before + 1);

    let journal_directory = find_dcommit_journal(&work);
    let store = JournalStore::new(&journal_directory);
    let journal = store.load().unwrap().expect("in-flight journal");
    assert!(matches!(
        journal.entries[0].state,
        EntryState::SubmissionInFlight { .. }
    ));
    assert_eq!(journal.target.mapping_ref, "refs/remotes/origin/main");
    assert_eq!(journal.target.commit_url, branch_url);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unfinished dcommit journal target does not match",
        ));
    assert_eq!(
        fixture.latest_revision(),
        submitted,
        "retrying without the bound commit URL must not resubmit"
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase", "--commit-url", &branch_url])
        .assert()
        .failure()
        .stderr(predicate::str::contains("outcome is ambiguous"));
    assert_eq!(
        fixture.latest_revision(),
        submitted,
        "automatic retry after a lost response must not resubmit"
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args([
            "dcommit",
            "--no-rebase",
            "--commit-url",
            &branch_url,
            "--adopt-revision",
            &submitted.to_string(),
        ])
        .assert()
        .success();
    assert_eq!(
        fixture.latest_revision(),
        submitted,
        "manual adoption must not submit another SVN revision"
    );
    let recovered = store.load().unwrap().expect("reconciled dcommit journal");
    assert_eq!(recovered.batch_state, BatchState::Complete);
    let imported_oid = match &recovered.entries[0].state {
        EntryState::FetchedVerified {
            svn_revision,
            imported_oid,
        } if *svn_revision == u64::from(submitted) => imported_oid,
        state => panic!("adopted commit URL was not verified: {state:?}"),
    };
    assert_eq!(recovered.target.mapping_ref, "refs/remotes/origin/main");
    assert_eq!(recovered.target.commit_url, branch_url);
    let branch_oid = git_stdout(&work, &["rev-parse", "refs/remotes/origin/main"]);
    assert_eq!(imported_oid, &branch_oid);
    let rev_map_path = std::path::PathBuf::from(&recovered.target.rev_map_path);
    let rev_map_path = if rev_map_path.is_absolute() {
        rev_map_path
    } else {
        work.join(rev_map_path)
    };
    assert_eq!(
        RevMap::open_existing(rev_map_path, ObjectFormat::Sha1)
            .unwrap()
            .get(submitted)
            .unwrap()
            .as_deref(),
        Some(branch_oid.as_str())
    );
    assert!(
        git_stdout(
            &work,
            &["show", "-s", "--format=%B", "refs/remotes/origin/main"]
        )
        .contains(&format!("{branch_url}@{submitted} "))
    );
    assert_eq!(
        svn_stdout(&[
            "cat",
            &format!("{}/branches/main/src/lib.rs", fixture.url())
        ]),
        "pub fn answer() -> u8 { 63 }\n"
    );
    assert_eq!(
        svn_stdout(&["cat", &format!("{}/trunk/src/lib.rs", fixture.url())]),
        "pub fn answer() -> u8 { 42 }\n"
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

#[cfg(unix)]
#[test]
fn dcommit_uses_git_askpass_for_authenticated_svnserve_write_and_fetch() {
    match require_svn_tools().and_then(|()| require_svnserve()) {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    use std::os::unix::fs::PermissionsExt;

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
    make_commit(
        &work,
        "askpass-write.txt",
        "authenticated by askpass\n",
        "askpass authenticated dcommit",
    );

    let askpass = parent.path().join("askpass");
    let askpass_log = parent.path().join("askpass.log");
    std::fs::write(
        &askpass,
        "#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"$GIT_SVN_RS_ASKPASS_LOG\"\nprintf 'do-not-leak' >&2\nexit 9\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&askpass).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&askpass, permissions).unwrap();

    let before = fixture.latest_revision();
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .env("GIT_ASKPASS", &askpass)
        .env("GIT_SVN_RS_ASKPASS_LOG", &askpass_log)
        .args([
            "dcommit",
            "--no-rebase",
            "--username",
            "alice",
            "--no-auth-cache",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("SVN askpass exited"))
        .stderr(predicate::str::contains("do-not-leak").not());
    assert_eq!(fixture.latest_revision(), before);
    assert!(
        !work
            .join(".git/svn/refs/remotes/origin/trunk/dcommit-journal")
            .exists()
    );

    std::fs::write(
        &askpass,
        "#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"$GIT_SVN_RS_ASKPASS_LOG\"\nprintf 'secret\\n'\n",
    )
    .unwrap();
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .env("GIT_ASKPASS", &askpass)
        .env("GIT_SVN_RS_ASKPASS_LOG", &askpass_log)
        .args([
            "dcommit",
            "--no-rebase",
            "--username",
            "alice",
            "--no-auth-cache",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("askpass authenticated dcommit"));

    assert_eq!(
        svn_stdout(&[
            "cat",
            &format!("{}/trunk/askpass-write.txt", server.repo_url())
        ]),
        "authenticated by askpass\n"
    );
    let prompts = std::fs::read_to_string(askpass_log).unwrap();
    assert_eq!(prompts.lines().count(), 3);
    assert!(prompts.lines().all(|line| line.contains("alice@svn://")));
    assert!(!prompts.contains("secret"));
    let git_config = std::fs::read_to_string(work.join(".git/config")).unwrap();
    assert!(!git_config.contains("secret"));
    assert!(!git_config.contains("password"));
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

#[cfg(unix)]
#[test]
fn dcommit_recovery_binds_authors_content_but_allows_password_rotation() {
    match require_svn_tools().and_then(|()| require_svnserve()) {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    use std::os::unix::fs::PermissionsExt;

    let fixture = StandardSvnFixture::create().unwrap();
    fixture
        .require_read_write_auth("alice", "old-secret")
        .unwrap();
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
            "old-secret",
            "--no-auth-cache",
        ])
        .assert()
        .success();

    let password_file = fixture.root().join("repo/conf/passwd");
    let authors_file = parent.path().join("authors.txt");
    std::fs::write(
        &authors_file,
        "alice = Alice Original <alice@example.com>\n",
    )
    .unwrap();
    let authors_file_arg = authors_file.to_string_lossy().into_owned();
    let hook = fixture.root().join("repo/hooks/post-commit");
    assert!(!password_file.to_string_lossy().contains('"'));
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\nprintf '[users]\\nalice = new-secret\\nbob = new-secret\\n' > \"{}\"\n",
            password_file.display()
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&hook).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&hook, permissions).unwrap();

    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );
    std::fs::write(work.join("src/lib.rs"), "pub fn answer() -> u8 { 52 }\n").unwrap();
    run_git(
        &work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-am",
            "rotate recovery password",
        ],
    );
    let before = fixture.latest_revision();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args([
            "dcommit",
            "--no-rebase",
            "--username",
            "alice",
            "--password",
            "old-secret",
            "--no-auth-cache",
            "--authors-file",
            &authors_file_arg,
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("post-submit"))
        .stderr(predicate::str::contains("old-secret").not());
    let submitted = fixture.latest_revision();
    assert_eq!(submitted, before + 1);

    let journal_directory = find_dcommit_journal(&work);
    let store = JournalStore::new(&journal_directory);
    let journal = store.load().unwrap().expect("submitted dcommit journal");
    assert!(matches!(
        journal.entries[0].state,
        EntryState::Submitted { .. }
    ));
    for entry in std::fs::read_dir(journal_directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let bytes = std::fs::read(path).unwrap();
            assert!(
                !bytes
                    .windows(b"old-secret".len())
                    .any(|v| v == b"old-secret")
            );
            assert!(
                !bytes
                    .windows(b"new-secret".len())
                    .any(|v| v == b"new-secret")
            );
        }
    }

    std::fs::write(&authors_file, "alice = Alice Changed <alice@example.com>\n").unwrap();
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args([
            "dcommit",
            "--no-rebase",
            "--username",
            "alice",
            "--password",
            "new-secret",
            "--no-auth-cache",
            "--authors-file",
            &authors_file_arg,
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "journal configuration does not match",
        ))
        .stderr(predicate::str::contains("new-secret").not());
    assert_eq!(
        fixture.latest_revision(),
        submitted,
        "changing authors-file content must not resubmit"
    );
    std::fs::write(
        &authors_file,
        "alice = Alice Original <alice@example.com>\n",
    )
    .unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args([
            "dcommit",
            "--no-rebase",
            "--username",
            "bob",
            "--password",
            "new-secret",
            "--no-auth-cache",
            "--authors-file",
            &authors_file_arg,
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "journal configuration does not match",
        ))
        .stderr(predicate::str::contains("new-secret").not());
    assert_eq!(
        fixture.latest_revision(),
        submitted,
        "changing the durable username intent must not resubmit"
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
            "new-secret",
            "--no-auth-cache",
            "--authors-file",
            &authors_file_arg,
        ])
        .assert()
        .success();
    assert_eq!(
        fixture.latest_revision(),
        submitted,
        "password rotation recovery must not resubmit"
    );
    assert_eq!(
        store.load().unwrap().unwrap().batch_state,
        BatchState::Complete
    );
}

#[test]
fn dcommit_auth_failure_stops_before_journal_or_svn_write() {
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
            "must not write with bad credentials",
        ],
    );
    let revision_before = svn_stdout(&[
        "--username",
        "alice",
        "--password",
        "secret",
        "--no-auth-cache",
        "--non-interactive",
        "info",
        "--show-item",
        "revision",
        &server.repo_url(),
    ]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args([
            "dcommit",
            "--no-rebase",
            "--username",
            "alice",
            "--password",
            "wrong",
            "--no-auth-cache",
        ])
        .assert()
        .failure();

    assert!(
        !work
            .join(".git/svn/refs/remotes/origin/trunk/dcommit-journal")
            .exists(),
        "authentication preflight must fail before creating a dcommit journal"
    );
    assert_eq!(
        svn_stdout(&[
            "--username",
            "alice",
            "--password",
            "secret",
            "--no-auth-cache",
            "--non-interactive",
            "info",
            "--show-item",
            "revision",
            &server.repo_url(),
        ]),
        revision_before,
        "authentication failure must not create an SVN revision"
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

    let revision_before = fixture.latest_revision();
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args([
            "dcommit",
            "--no-rebase",
            "--commit-url",
            &format!("{}/branches/untracked", fixture.url()),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "does not match a tracked SVN mapping",
        ));
    assert_eq!(fixture.latest_revision(), revision_before);
    assert!(
        !work
            .join(".git/svn/refs/remotes/origin/trunk/dcommit-journal")
            .exists()
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

    let store = JournalStore::new(find_dcommit_journal(&work));
    let journal = store.load().unwrap().expect("completed dcommit journal");
    let (revision, imported_oid) = match &journal.entries[0].state {
        EntryState::FetchedVerified {
            svn_revision,
            imported_oid,
        } => (*svn_revision, imported_oid.clone()),
        state => panic!("explicit commit URL was not verified: {state:?}"),
    };
    assert_eq!(journal.target.mapping_ref, "refs/remotes/origin/main");
    let branch_oid = git_stdout(&work, &["rev-parse", "refs/remotes/origin/main"]);
    let trunk_oid = git_stdout(&work, &["rev-parse", "refs/remotes/origin/trunk"]);
    assert_eq!(imported_oid, branch_oid);
    assert_ne!(imported_oid, trunk_oid);
    let rev_map_path = std::path::PathBuf::from(&journal.target.rev_map_path);
    let rev_map_path = if rev_map_path.is_absolute() {
        rev_map_path
    } else {
        work.join(rev_map_path)
    };
    assert_eq!(
        RevMap::open_existing(rev_map_path, ObjectFormat::Sha1)
            .unwrap()
            .get(u32::try_from(revision).unwrap())
            .unwrap()
            .as_deref(),
        Some(branch_oid.as_str())
    );
    assert!(
        git_stdout(
            &work,
            &["show", "-s", "--format=%B", "refs/remotes/origin/main"]
        )
        .contains(&format!("{branch_url}@{revision} "))
    );

    let mut interrupted = journal;
    interrupted.batch_state = BatchState::Submitting;
    interrupted.entries[0].state = EntryState::Submitted {
        svn_revision: revision,
    };
    {
        let lock = store.acquire_lock().unwrap();
        store.save(&lock, &interrupted).unwrap();
    }
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase", "--commit-url", &branch_url])
        .assert()
        .success();
    assert_eq!(
        fixture.latest_revision(),
        u32::try_from(revision).unwrap(),
        "resuming explicit commit URL verification must not resubmit"
    );
}

#[test]
fn dcommit_rejects_stale_explicit_target_before_journal_or_svn_write() {
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
    make_commit(
        &work,
        "src/lib.rs",
        "pub fn answer() -> u8 { 100 }\n",
        "local stale commit",
    );
    let local_head = git_stdout(&work, &["rev-parse", "HEAD"]);

    let upstream = temp.path().join("upstream");
    let branch_url = format!("{}/branches/main", fixture.url());
    svn_command(
        temp.path(),
        &["checkout", "--non-interactive", &branch_url, "upstream"],
    );
    std::fs::write(upstream.join("upstream.txt"), "advanced elsewhere\n").unwrap();
    svn_command(&upstream, &["add", "--non-interactive", "upstream.txt"]);
    svn_command(
        &upstream,
        &[
            "commit",
            "--non-interactive",
            "-m",
            "advance mapped branch externally",
        ],
    );
    let advanced_revision = fixture.latest_revision();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase", "--commit-url", &branch_url])
        .assert()
        .failure()
        .stderr(predicate::str::contains("SVN remote advanced"))
        .stderr(predicate::str::contains("refusing to submit"));

    assert_eq!(
        fixture.latest_revision(),
        advanced_revision,
        "stale-head rejection must not create an SVN revision"
    );
    assert_eq!(git_stdout(&work, &["rev-parse", "HEAD"]), local_head);
    assert_eq!(
        svn_stdout(&["cat", &format!("{branch_url}/src/lib.rs")]),
        "pub fn answer() -> u8 { 42 }\n"
    );
    assert!(
        !work
            .join(".git/svn/refs/remotes/origin/main/dcommit-journal")
            .exists(),
        "stale-head preflight must fail before creating a dcommit journal"
    );
}

#[test]
fn dcommit_rejects_commit_url_outside_configured_remote_before_write() {
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
    let other = StandardSvnFixture::create().unwrap();
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
    std::fs::write(work.join("src/lib.rs"), "pub fn answer() -> u8 { 100 }\n").unwrap();
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
            "must not reach another repository",
        ],
    );

    let other_revision = svn_stdout(&["info", "--show-item", "revision", &other.url()]);
    let other_branch_url = format!("{}/branches/main", other.url());
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--no-rebase", "--commit-url", &other_branch_url])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "outside the configured SVN remote",
        ));

    assert_eq!(
        svn_stdout(&["info", "--show-item", "revision", &other.url()]),
        other_revision,
        "a mismatched commit URL must not create an SVN revision"
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
    set_executable(&work.join("tool.sh"), true);
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
    set_executable(&work.join("run.sh"), false);
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
        .stderr(predicate::str::contains("HTTP(S) SVN dcommit"))
        .stderr(predicate::str::contains("before recovery"));
    assert!(
        !work
            .join(".git/svn/refs/remotes/git-svn/dcommit-journal")
            .exists(),
        "unsupported remote profiles must fail before journal setup"
    );
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
    let metadata = work.join(".git/svn/refs/remotes/git-svn");
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

fn find_dcommit_journal(work: &std::path::Path) -> std::path::PathBuf {
    let svn_metadata = work.join(".git/svn");
    let mut directories = vec![svn_metadata];
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if !path.is_dir() {
                continue;
            }
            if path
                .file_name()
                .is_some_and(|name| name == "dcommit-journal")
            {
                return path;
            }
            directories.push(path);
        }
    }
    panic!("dcommit journal directory not found")
}

fn write_mock_dcommit_journal(
    work: &std::path::Path,
    tracked_oid: &str,
    head_oid: &str,
    batch_state: BatchState,
    entry_state: EntryState,
) {
    let metadata = work.join(".git/svn/refs/remotes/git-svn");
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

fn set_executable(path: &std::path::Path, executable: bool) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        let mode = permissions.mode();
        permissions.set_mode(if executable {
            mode | 0o111
        } else {
            mode & !0o111
        });
        std::fs::set_permissions(path, permissions).unwrap();
    }
    #[cfg(not(unix))]
    let _ = (path, executable);
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

fn svn_command(cwd: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("svn")
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "svn {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
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
