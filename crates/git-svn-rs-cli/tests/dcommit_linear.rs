use assert_cmd::Command;
use predicates::prelude::*;

#[allow(dead_code)]
#[path = "../../git-svn-rs-core/tests/support/svn_fixture.rs"]
mod svn_fixture;

use svn_fixture::{StandardSvnFixture, SvnToolPolicy, require_svn_tools};

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
    assert!(String::from_utf8_lossy(&output.stdout).contains("change answer"));
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
