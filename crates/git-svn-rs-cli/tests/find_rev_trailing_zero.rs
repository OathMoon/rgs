use assert_cmd::Command;
use git_svn_rs_core::rev_map::{ObjectFormat, RevMap};

#[test]
fn trailing_zero_marker_does_not_hide_the_nearest_before_revision() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", "mock://repo", "work", "--stdlayout"])
        .assert()
        .success();

    let tracking_ref = "refs/remotes/origin/trunk";
    let tracked_oid = git_stdout(&work, &["rev-parse", tracking_ref]);
    let rev_map_path = work
        .join(".git/svn/refs/remotes/origin/trunk")
        .join(".rev_map.mock-uuid");
    let mut rev_map = RevMap::open_existing(&rev_map_path, ObjectFormat::Sha1).unwrap();
    rev_map.append(3, &"0".repeat(40)).unwrap();
    let rev_map_before = std::fs::read(&rev_map_path).unwrap();
    let ref_before = git_stdout(&work, &["rev-parse", tracking_ref]);
    let config_before = std::fs::read(work.join(".git/config")).unwrap();

    for revision in ["r3", "r4"] {
        Command::cargo_bin("git-svn-rs")
            .unwrap()
            .current_dir(&work)
            .args(["find-rev", "--before", revision])
            .assert()
            .success()
            .stdout(format!("{tracked_oid}\n"));
    }
    for args in [&["find-rev", "r3"][..], &["find-rev", "--after", "r3"][..]] {
        Command::cargo_bin("git-svn-rs")
            .unwrap()
            .current_dir(&work)
            .args(args)
            .assert()
            .success()
            .stdout("");
    }

    assert_eq!(std::fs::read(rev_map_path).unwrap(), rev_map_before);
    assert_eq!(git_stdout(&work, &["rev-parse", tracking_ref]), ref_before);
    assert_eq!(
        std::fs::read(work.join(".git/config")).unwrap(),
        config_before
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
