use assert_cmd::Command;

#[allow(dead_code)]
#[path = "../../git-svn-rs-core/tests/support/svn_fixture.rs"]
mod svn_fixture;

use svn_fixture::{StandardSvnFixture, SvnToolPolicy, require_svn_tools};

#[test]
fn clone_stdlayout_file_url_imports_trunk_history() {
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

    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:src/lib.rs"])
            .unwrap(),
        "pub fn answer() -> u8 { 42 }\n".to_string()
    );

    let commit = git
        .run_for_test(["show", "-s", "--format=%B", "refs/remotes/origin/trunk"])
        .unwrap();
    assert!(commit.contains("add trunk file"));
    assert!(commit.contains("git-svn-id: "));
    assert!(commit.contains(&format!("{}/trunk@2", fixture.url().trim_end_matches('/'))));

    let rev_map_dir = work.join(".git").join("svn").join("origin.trunk");
    let rev_map = std::fs::read_dir(&rev_map_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".rev_map."))
        })
        .expect("origin/trunk rev_map should be written");
    assert!(std::fs::metadata(rev_map).unwrap().len() > 0);
}

#[test]
fn fetch_stdlayout_file_url_imports_trunk_history_after_init() {
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
        .args(["init", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:src/lib.rs"])
            .unwrap(),
        "pub fn answer() -> u8 { 42 }\n".to_string()
    );
}

#[test]
fn clone_stdlayout_file_url_imports_branch_tag_and_copy_contents() {
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

    let git = git_svn_rs_core::git::GitCli::new(&work);
    let refs = git
        .run_for_test(["for-each-ref", "--format=%(refname)", "refs/remotes/origin"])
        .unwrap();
    assert!(refs.lines().any(|line| line == "refs/remotes/origin/trunk"));
    assert!(refs.lines().any(|line| line == "refs/remotes/origin/main"));
    assert!(
        refs.lines()
            .any(|line| line == "refs/remotes/origin/tags/v1")
    );

    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/main:src/lib.rs"])
            .unwrap(),
        "pub fn answer() -> u8 { 42 }\n".to_string()
    );
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/tags/v1:src/lib.rs"])
            .unwrap(),
        "pub fn answer() -> u8 { 42 }\n".to_string()
    );
    assert_eq!(
        git.run_for_test(["ls-tree", "refs/remotes/origin/trunk", "run.sh"])
            .unwrap()
            .split_whitespace()
            .next(),
        Some("100755")
    );
    assert_eq!(
        git.run_for_test(["ls-tree", "refs/remotes/origin/trunk", "link-to-lib"])
            .unwrap()
            .split_whitespace()
            .next(),
        Some("120000")
    );
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:link-to-lib"])
            .unwrap(),
        "src/lib.rs".to_string()
    );

    let trunk_tree = git
        .run_for_test(["ls-tree", "-r", "--name-only", "refs/remotes/origin/trunk"])
        .unwrap();
    assert!(!trunk_tree.lines().any(|line| line == "empty-dir"));
}
