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

#[test]
fn clone_stdlayout_file_url_preserves_empty_dirs_when_requested() {
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
        .args([
            "clone",
            &fixture.url(),
            "work",
            "--stdlayout",
            "--preserve-empty-dirs",
            "--placeholder-filename",
            ".gitkeep",
        ])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:empty-dir/.gitkeep"])
            .unwrap(),
        String::new()
    );
}

#[test]
fn fetch_file_url_preserves_empty_dirs_from_persisted_config() {
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
        .args([
            "init",
            &fixture.url(),
            "work",
            "--stdlayout",
            "--preserve-empty-dirs",
            "--placeholder-filename",
            ".gitkeep",
        ])
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
        git.run_for_test(["show", "refs/remotes/origin/trunk:empty-dir/.gitkeep"])
            .unwrap(),
        String::new()
    );
}

#[test]
fn clone_file_url_applies_ignore_paths_filter() {
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
        .args([
            "clone",
            &fixture.url(),
            "work",
            "--stdlayout",
            "--ignore-paths",
            "^trunk/run\\.sh$",
        ])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    let trunk_tree = git
        .run_for_test(["ls-tree", "-r", "--name-only", "refs/remotes/origin/trunk"])
        .unwrap();
    assert!(!trunk_tree.lines().any(|line| line == "run.sh"));
    assert!(trunk_tree.lines().any(|line| line == "src/lib.rs"));
}

#[test]
fn clone_file_url_applies_include_paths_filter() {
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
        .args([
            "clone",
            &fixture.url(),
            "work",
            "--stdlayout",
            "--include-paths",
            "^trunk/src/",
        ])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    let trunk_tree = git
        .run_for_test(["ls-tree", "-r", "--name-only", "refs/remotes/origin/trunk"])
        .unwrap();
    assert!(trunk_tree.lines().any(|line| line == "src/lib.rs"));
    assert!(!trunk_tree.lines().any(|line| line == "run.sh"));
    assert!(!trunk_tree.lines().any(|line| line == "link-to-lib"));
}

#[test]
fn clone_file_url_applies_authors_file_mapping() {
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
    let svn_author = svn_stdout(&["log", "-r", "2", "--quiet", &fixture.url()])
        .lines()
        .find_map(|line| {
            let parts = line.split('|').map(str::trim).collect::<Vec<_>>();
            (parts.len() >= 2 && parts[0].starts_with('r')).then(|| parts[1].to_string())
        })
        .expect("svn log should include an author");
    let authors = temp.path().join("authors.txt");
    std::fs::write(
        &authors,
        format!("{svn_author} = Ada Lovelace <ada@example.com>\n"),
    )
    .unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "clone",
            &fixture.url(),
            "work",
            "--stdlayout",
            "--authors-file",
            authors.to_str().unwrap(),
        ])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert_eq!(
        git.run_for_test([
            "show",
            "-s",
            "--format=%an <%ae>",
            "refs/remotes/origin/trunk"
        ])
        .unwrap()
        .trim(),
        "Ada Lovelace <ada@example.com>"
    );
}

#[test]
fn clone_file_url_honors_revision_range_end() {
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
        .args([
            "clone",
            &fixture.url(),
            "work",
            "--stdlayout",
            "--revision",
            "1:2",
        ])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    let refs = git
        .run_for_test(["for-each-ref", "--format=%(refname)", "refs/remotes/origin"])
        .unwrap();
    assert!(refs.lines().any(|line| line == "refs/remotes/origin/trunk"));
    assert!(!refs.lines().any(|line| line == "refs/remotes/origin/main"));
    assert!(
        !refs
            .lines()
            .any(|line| line == "refs/remotes/origin/tags/v1")
    );
    let trunk_tree = git
        .run_for_test(["ls-tree", "-r", "--name-only", "refs/remotes/origin/trunk"])
        .unwrap();
    assert!(trunk_tree.lines().any(|line| line == "src/lib.rs"));
    assert!(trunk_tree.lines().any(|line| line == "run.sh"));
}

#[test]
fn fetch_file_url_applies_persisted_authors_file_mapping() {
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
    let svn_author = fixture_author(&fixture);
    let authors = temp.path().join("authors.txt");
    std::fs::write(
        &authors,
        format!("{svn_author} = Grace Hopper <grace@example.com>\n"),
    )
    .unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "init",
            &fixture.url(),
            "work",
            "--stdlayout",
            "--authors-file",
            authors.to_str().unwrap(),
        ])
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
        git.run_for_test([
            "show",
            "-s",
            "--format=%an <%ae>",
            "refs/remotes/origin/trunk"
        ])
        .unwrap()
        .trim(),
        "Grace Hopper <grace@example.com>"
    );
}

fn fixture_author(fixture: &StandardSvnFixture) -> String {
    svn_stdout(&["log", "-r", "2", "--quiet", &fixture.url()])
        .lines()
        .find_map(|line| {
            let parts = line.split('|').map(str::trim).collect::<Vec<_>>();
            (parts.len() >= 2 && parts[0].starts_with('r')).then(|| parts[1].to_string())
        })
        .expect("svn log should include an author")
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
    String::from_utf8_lossy(&output.stdout).to_string()
}
