use assert_cmd::Command;
use git_svn_rs_core::rev_map::{ObjectFormat, RevMap};
#[cfg(unix)]
use predicates::prelude::PredicateBooleanExt;

#[allow(dead_code)]
#[path = "../../git-svn-rs-core/tests/support/svn_fixture.rs"]
mod svn_fixture;

#[cfg(unix)]
use svn_fixture::{HttpDav, require_http_dav};
use svn_fixture::{
    StandardSvnFixture, SvnServe, SvnToolPolicy, require_svn_tools, require_svnserve,
};

#[test]
fn clone_and_fetch_peg_sensitive_file_url() {
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
    let initial_revision = fixture.create_peg_sensitive_trunk().unwrap();
    let url = format!("{}/trunk%40main", fixture.url());
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "clone",
            &url,
            "work",
            "--revision",
            &format!("{initial_revision}:HEAD"),
        ])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/git-svn:run.sh"])
            .unwrap(),
        "#!/bin/sh\necho hi\n"
    );
    assert!(
        git.run_for_test(["show", "-s", "--format=%B", "refs/remotes/git-svn"])
            .unwrap()
            .contains(&format!("trunk%40main@{initial_revision} "))
    );

    let fetched_revision = fixture
        .modify_peg_sensitive_run_script("#!/bin/sh\necho peg\n")
        .unwrap();
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
        .assert()
        .success();
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/git-svn:run.sh"])
            .unwrap(),
        "#!/bin/sh\necho peg\n"
    );
    assert!(
        git.run_for_test(["show", "-s", "--format=%B", "refs/remotes/git-svn"])
            .unwrap()
            .contains(&format!("trunk%40main@{fetched_revision} "))
    );
}

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

    let rev_map_dir = work.join(".git/svn/refs/remotes/origin/trunk");
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
fn partial_layout_arguments_match_frozen_mapping_selection() {
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

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "init",
            &fixture.url(),
            "branches-only",
            "--branches",
            "branches/*",
        ])
        .assert()
        .success();
    let branches_only = temp.path().join("branches-only");
    let git = git_svn_rs_core::git::GitCli::new(&branches_only);
    assert!(
        git.config_get_all("svn-remote.svn.fetch")
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        git.config_get_all("svn-remote.svn.branches").unwrap(),
        vec!["branches/*:refs/remotes/origin/*"]
    );
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&branches_only)
        .arg("fetch")
        .assert()
        .success();
    assert!(
        git.run_for_test(["rev-parse", "--verify", "refs/remotes/origin/main"])
            .is_ok()
    );
    assert!(
        git.run_for_test(["rev-parse", "--verify", "refs/remotes/origin/trunk"])
            .is_err()
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "init",
            &fixture.url(),
            "partial-stdlayout",
            "--stdlayout",
            "--branches",
            "other/*",
        ])
        .assert()
        .success();
    let partial_stdlayout = temp.path().join("partial-stdlayout");
    let git = git_svn_rs_core::git::GitCli::new(&partial_stdlayout);
    assert_eq!(
        git.config_get_all("svn-remote.svn.fetch").unwrap(),
        vec!["trunk:refs/remotes/origin/trunk"]
    );
    assert_eq!(
        git.config_get_all("svn-remote.svn.tags").unwrap(),
        vec!["tags/*:refs/remotes/origin/tags/*"]
    );
}

#[test]
fn full_url_layout_arguments_are_repository_relative_and_same_repository() {
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
    let trunk = format!("{}/trunk", fixture.url());
    let branches = format!("{}/branches", fixture.url());
    let tags = format!("{}/tags", fixture.url());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "init",
            &fixture.url(),
            "full-layout",
            "--trunk",
            &trunk,
            "--branches",
            &branches,
            "--tags",
            &tags,
        ])
        .assert()
        .success();
    let work = temp.path().join("full-layout");
    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert_eq!(
        git.config_get_all("svn-remote.svn.url").unwrap(),
        vec![fixture.url()]
    );
    assert_eq!(
        git.config_get_all("svn-remote.svn.fetch").unwrap(),
        vec!["trunk:refs/remotes/origin/trunk"]
    );
    assert_eq!(
        git.config_get_all("svn-remote.svn.branches").unwrap(),
        vec!["branches/*:refs/remotes/origin/*"]
    );
    assert_eq!(
        git.config_get_all("svn-remote.svn.tags").unwrap(),
        vec!["tags/*:refs/remotes/origin/tags/*"]
    );
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
        .assert()
        .success();
    for refname in [
        "refs/remotes/origin/trunk",
        "refs/remotes/origin/main",
        "refs/remotes/origin/tags/v1",
    ] {
        assert!(
            git.run_for_test(["rev-parse", "--verify", refname]).is_ok(),
            "full URL layout did not import {refname}"
        );
    }

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "clone",
            &fixture.url(),
            "full-url-clone",
            "--trunk",
            &trunk,
            "--branches",
            &branches,
            "--tags",
            &tags,
        ])
        .assert()
        .success();
    let clone = git_svn_rs_core::git::GitCli::new(temp.path().join("full-url-clone"));
    assert_eq!(
        clone.run_for_test(["show", "HEAD:src/lib.rs"]).unwrap(),
        "pub fn answer() -> u8 { 42 }\n"
    );

    let other = StandardSvnFixture::create().unwrap();
    let outside_trunk = format!("{}/trunk", other.url());
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "init",
            &fixture.url(),
            "outside-layout",
            "--trunk",
            &outside_trunk,
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("outside repository root"));
    assert!(!temp.path().join("outside-layout").exists());
}

#[test]
fn clone_stdlayout_replays_bounded_log_windows_without_losing_mappings() {
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
            "--log-window-size",
            "1",
        ])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    for refname in [
        "refs/remotes/origin/trunk",
        "refs/remotes/origin/main",
        "refs/remotes/origin/tags/v1",
    ] {
        assert!(
            git.run_for_test(["rev-parse", "--verify", refname]).is_ok(),
            "missing mapping after bounded replay: {refname}"
        );
    }
    assert_copy_parent_matches_trunk_revision(&git, &work, 1);
}

#[test]
fn clone_branch_range_backfills_copy_source_history_for_parent_identity() {
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
            "3:3",
        ])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/main:src/lib.rs"])
            .unwrap(),
        "pub fn answer() -> u8 { 42 }\n"
    );
    assert_copy_parent_matches_trunk_revision(&git, &work, 1);
}

#[test]
fn fetch_reuses_auxiliary_branch_revision_ref_for_unmapped_copy_source() {
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
    let upstream = temp.path().join("upstream");
    run_svn(temp.path(), &["checkout", &fixture.url(), "upstream"]);
    std::fs::create_dir_all(upstream.join("legacy")).unwrap();
    std::fs::write(upstream.join("legacy/file.txt"), "legacy\n").unwrap();
    run_svn(&upstream, &["add", "--non-interactive", "legacy"]);
    run_svn(
        &upstream,
        &[
            "commit",
            "--non-interactive",
            "-m",
            "create unmapped legacy source",
        ],
    );
    let source_revision = svn_stdout(&["info", "--show-item", "revision", &fixture.url()])
        .trim()
        .parse::<u32>()
        .unwrap();
    run_svn(
        &upstream,
        &["copy", "--non-interactive", "legacy", "branches/external"],
    );
    run_svn(
        &upstream,
        &[
            "commit",
            "--non-interactive",
            "-m",
            "copy unmapped source into branch layout",
        ],
    );
    let copy_revision = svn_stdout(&["info", "--show-item", "revision", &fixture.url()])
        .trim()
        .parse::<u32>()
        .unwrap();

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
        .args([
            "fetch",
            "--revision",
            &format!("{copy_revision}:{copy_revision}"),
        ])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    let auxiliary = format!("refs/remotes/origin/external@{source_revision}");
    assert!(
        git.config_get_all("svn-remote.svn.fetch")
            .unwrap()
            .iter()
            .all(|mapping| !mapping.contains(&auxiliary))
    );
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/external:file.txt"])
            .unwrap(),
        "legacy\n"
    );
    assert_eq!(
        git.run_for_test(["rev-parse", "refs/remotes/origin/external^"])
            .unwrap()
            .trim(),
        git.run_for_test(["rev-parse", &auxiliary]).unwrap().trim()
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args([
            "fetch",
            "--revision",
            &format!("{copy_revision}:{copy_revision}"),
        ])
        .assert()
        .success();
    assert!(git.rev_parse(&format!("{auxiliary}-")).is_err());
}

#[test]
fn clone_discovers_branches_inside_an_ancestor_directory_copy() {
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
    let upstream = temp.path().join("upstream");
    run_svn(temp.path(), &["checkout", &fixture.url(), "upstream"]);
    std::fs::create_dir_all(upstream.join("archive/promoted/a")).unwrap();
    std::fs::write(
        upstream.join("archive/promoted/a/file.txt"),
        "ancestor copy\n",
    )
    .unwrap();
    run_svn(&upstream, &["add", "--non-interactive", "archive"]);
    run_svn(
        &upstream,
        &[
            "commit",
            "--non-interactive",
            "-m",
            "create archived branch layout",
        ],
    );
    let source_revision = svn_stdout(&["info", "--show-item", "revision", &fixture.url()])
        .trim()
        .parse::<u32>()
        .unwrap();
    run_svn(
        &upstream,
        &["copy", "--non-interactive", "archive/promoted", "promoted"],
    );
    run_svn(
        &upstream,
        &[
            "commit",
            "--non-interactive",
            "-m",
            "promote archived branch layout",
        ],
    );

    let work = temp.path().join("work");
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "clone",
            &fixture.url(),
            "work",
            "--trunk",
            "trunk",
            "--branches",
            "promoted/*",
        ])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    let destination = "refs/remotes/origin/a";
    let auxiliary = format!("{destination}@{source_revision}");
    assert_eq!(
        git.run_for_test(["show", &format!("{destination}:file.txt")])
            .unwrap(),
        "ancestor copy\n"
    );
    assert_eq!(
        git.run_for_test(["rev-parse", &format!("{destination}^")])
            .unwrap()
            .trim(),
        git.run_for_test(["rev-parse", &auxiliary]).unwrap().trim()
    );
}

#[test]
fn clone_discovers_empty_nested_branch_inside_an_ancestor_copy() {
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
    let upstream = temp.path().join("upstream");
    run_svn(temp.path(), &["checkout", &fixture.url(), "upstream"]);
    std::fs::create_dir_all(upstream.join("archive/projects/acme/main")).unwrap();
    run_svn(&upstream, &["add", "--non-interactive", "archive"]);
    run_svn(
        &upstream,
        &[
            "commit",
            "--non-interactive",
            "-m",
            "create archived empty nested branch",
        ],
    );
    let source_revision = svn_stdout(&["info", "--show-item", "revision", &fixture.url()])
        .trim()
        .parse::<u32>()
        .unwrap();
    run_svn(
        &upstream,
        &["copy", "--non-interactive", "archive/projects", "projects"],
    );
    run_svn(
        &upstream,
        &[
            "commit",
            "--non-interactive",
            "-m",
            "promote empty nested branches",
        ],
    );

    let work = temp.path().join("work");
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "clone",
            &fixture.url(),
            "work",
            "--branches",
            "projects/*/*",
            "--prefix",
            "origin/",
            "--preserve-empty-dirs",
        ])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    let destination = "refs/remotes/origin/acme/main";
    let auxiliary = format!("{destination}@{source_revision}");
    assert!(git.rev_parse(destination).is_ok());
    assert!(git.rev_parse(&auxiliary).is_ok());
    assert_eq!(
        git.run_for_test(["ls-tree", "-r", "--name-only", destination])
            .unwrap(),
        ""
    );
}

#[test]
fn fetch_persists_monotonic_branch_and_tag_discovery_high_water() {
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
    let head = svn_stdout(&["info", "--show-item", "revision", &fixture.url()])
        .trim()
        .parse::<u32>()
        .unwrap();
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
    for kind in ["branches", "tags"] {
        let key = format!("svn-remote.svn.{kind}-maxRev");
        assert_eq!(
            git.git_svn_metadata_get(&key).unwrap(),
            Some(head.to_string())
        );
        assert_eq!(git.config_get(&key).unwrap(), None);
    }

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["fetch", "--revision", "1:2"])
        .assert()
        .success();
    assert_eq!(
        git.git_svn_metadata_get("svn-remote.svn.branches-maxRev")
            .unwrap(),
        Some(head.to_string())
    );
}

#[test]
fn fetch_records_and_replaces_the_fixed_mapping_scan_marker() {
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

    let initial_head = svn_stdout(&["info", "--show-item", "revision", &fixture.url()])
        .trim()
        .parse::<u32>()
        .unwrap();
    let path = trunk_rev_map_path(&work);
    let map = RevMap::open_existing(&path, ObjectFormat::Sha1).unwrap();
    let records = map.records().unwrap();
    assert_eq!(records.last().unwrap().revision, initial_head);
    assert!(
        records
            .last()
            .unwrap()
            .object_id_hex
            .bytes()
            .all(|byte| byte == b'0')
    );
    let initial_len = records.len();
    let initial_bytes = std::fs::read(&path).unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
        .assert()
        .success();
    assert_eq!(std::fs::read(&path).unwrap(), initial_bytes);

    let upstream = temp.path().join("upstream");
    run_svn(temp.path(), &["checkout", &fixture.url(), "upstream"]);
    std::fs::write(
        upstream.join("trunk/src/lib.rs"),
        "pub fn answer() -> u8 { 43 }\n",
    )
    .unwrap();
    run_svn(
        &upstream,
        &["commit", "--non-interactive", "-m", "update trunk"],
    );
    let updated_head = svn_stdout(&["info", "--show-item", "revision", &fixture.url()])
        .trim()
        .parse::<u32>()
        .unwrap();
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
        .assert()
        .success();

    let records = RevMap::open_existing(&path, ObjectFormat::Sha1)
        .unwrap()
        .records()
        .unwrap();
    assert_eq!(records.len(), initial_len);
    assert_eq!(records.last().unwrap().revision, updated_head);
    assert!(
        records
            .last()
            .unwrap()
            .object_id_hex
            .bytes()
            .any(|byte| byte != b'0')
    );
}

#[test]
fn clone_single_subdirectory_file_url_imports_session_root() {
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
    let trunk_url = format!("{}/trunk", fixture.url().trim_end_matches('/'));

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &trunk_url, "work"])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/git-svn:src/lib.rs"])
            .unwrap(),
        "pub fn answer() -> u8 { 42 }\n"
    );
    assert!(
        git.run_for_test(["show", "refs/remotes/git-svn:trunk/src/lib.rs"])
            .is_err()
    );
    assert_eq!(
        git.run_for_test(["rev-parse", "HEAD"]).unwrap(),
        git.run_for_test(["rev-parse", "refs/remotes/git-svn"])
            .unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(work.join("src/lib.rs")).unwrap(),
        "pub fn answer() -> u8 { 42 }\n"
    );
}

#[test]
fn clone_stdlayout_svn_url_imports_trunk_history() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }
    match require_svnserve() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let server = SvnServe::start(fixture.root()).unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &server.repo_url(), "work", "--stdlayout"])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:src/lib.rs"])
            .unwrap(),
        "pub fn answer() -> u8 { 42 }\n".to_string()
    );
}

#[cfg(unix)]
#[test]
fn clone_and_incremental_fetch_use_authenticated_http_dav() {
    match require_svn_tools().and_then(|()| require_http_dav()) {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let server = HttpDav::start_basic(fixture.root(), "alice", "secret").unwrap();
    let denied_work = temp.path().join("denied");
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &server.repo_url(), "denied", "--stdlayout"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("secret").not());
    assert!(!denied_work.join(".git/svn").exists());

    let work = temp.path().join("work");
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
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

    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:src/lib.rs"])
            .unwrap(),
        "pub fn answer() -> u8 { 42 }\n"
    );
    let before = git
        .run_for_test(["rev-parse", "refs/remotes/origin/trunk"])
        .unwrap();

    fixture
        .modify_run_script_content("#!/bin/sh\necho http\n")
        .unwrap();
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args([
            "fetch",
            "--username",
            "alice",
            "--password",
            "secret",
            "--no-auth-cache",
        ])
        .assert()
        .success();

    let after = git
        .run_for_test(["rev-parse", "refs/remotes/origin/trunk"])
        .unwrap();
    assert_ne!(after, before);
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:run.sh"])
            .unwrap(),
        "#!/bin/sh\necho http\n"
    );
    assert!(
        git.run_for_test(["show", "-s", "--format=%B", "refs/remotes/origin/trunk"])
            .unwrap()
            .contains(&format!("git-svn-id: {}/trunk@5 ", server.repo_url()))
    );
}

#[cfg(unix)]
#[test]
fn clone_and_incremental_fetch_use_real_svn_ssh_tunnel() {
    match require_svn_tools().and_then(|()| require_svnserve()) {
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
    let wrapper = temp.path().join("svn-ssh");
    let tunnel_log = temp.path().join("svn-ssh.args");
    std::fs::write(
        &wrapper,
        concat!(
            "#!/bin/sh\n",
            "printf '%s\\n' \"$@\" >> \"$GIT_SVN_RS_SSH_ARG_LOG\"\n",
            "[ \"$#\" -eq 3 ] && [ \"$1\" = fixture ] && ",
            "[ \"$2\" = svnserve ] && [ \"$3\" = -t ] || exit 64\n",
            "exec svnserve -t --tunnel-user=fixture -r \"$GIT_SVN_RS_SVN_ROOT\"\n",
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&wrapper).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&wrapper, permissions).unwrap();
    let config_dir = temp.path().join("svn-config");
    std::fs::create_dir(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config"),
        format!("[tunnels]\nssh = {}\n", wrapper.display()),
    )
    .unwrap();

    let work = temp.path().join("work");
    let url = "svn+ssh://fixture/repo/trunk";
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .env("GIT_SVN_RS_SVN_ROOT", fixture.root())
        .env("GIT_SVN_RS_SSH_ARG_LOG", &tunnel_log)
        .args([
            "clone",
            url,
            "work",
            "--config-dir",
            config_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/git-svn:src/lib.rs"])
            .unwrap(),
        "pub fn answer() -> u8 { 42 }\n"
    );
    let before = git
        .run_for_test(["rev-parse", "refs/remotes/git-svn"])
        .unwrap();

    fixture
        .modify_run_script_content("#!/bin/sh\necho tunneled\n")
        .unwrap();
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .env("GIT_SVN_RS_SVN_ROOT", fixture.root())
        .env("GIT_SVN_RS_SSH_ARG_LOG", &tunnel_log)
        .arg("fetch")
        .assert()
        .success();

    let after = git
        .run_for_test(["rev-parse", "refs/remotes/git-svn"])
        .unwrap();
    assert_ne!(after, before);
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/git-svn:run.sh"])
            .unwrap(),
        "#!/bin/sh\necho tunneled\n"
    );
    assert!(
        git.run_for_test(["show", "-s", "--format=%B", "refs/remotes/git-svn"])
            .unwrap()
            .contains("git-svn-id: svn+ssh://fixture/repo/trunk@5 ")
    );
    let tunnel_args = std::fs::read_to_string(tunnel_log).unwrap();
    let lines = tunnel_args.lines().collect::<Vec<_>>();
    assert!(!lines.is_empty());
    assert!(
        lines
            .chunks_exact(3)
            .all(|args| args == ["fixture", "svnserve", "-t"])
    );
}

#[test]
fn clone_stdlayout_authenticated_svn_url_imports_with_password() {
    match require_svn_tools().and_then(|()| require_svnserve()) {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    fixture.require_basic_auth("alice", "secret").unwrap();
    let server = SvnServe::start(fixture.root()).unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
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

    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:src/lib.rs"])
            .unwrap(),
        "pub fn answer() -> u8 { 42 }\n".to_string()
    );
}

#[cfg(unix)]
#[test]
fn clone_and_fetch_authenticated_svn_url_use_git_askpass() {
    match require_svn_tools().and_then(|()| require_svnserve()) {
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
    fixture.require_basic_auth("alice", "secret").unwrap();
    let server = SvnServe::start(fixture.root()).unwrap();
    let askpass = temp.path().join("askpass");
    let askpass_log = temp.path().join("askpass.log");
    std::fs::write(
        &askpass,
        "#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"$GIT_SVN_RS_ASKPASS_LOG\"\nprintf 'secret\\n'\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&askpass).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&askpass, permissions).unwrap();

    let work = temp.path().join("work");
    let trunk_url = format!("{}/trunk", server.repo_url());
    let branches_url = format!("{}/branches", server.repo_url());
    let tags_url = format!("{}/tags", server.repo_url());
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .env("GIT_ASKPASS", &askpass)
        .env("GIT_SVN_RS_ASKPASS_LOG", &askpass_log)
        .args([
            "clone",
            &server.repo_url(),
            "work",
            "--trunk",
            &trunk_url,
            "--branches",
            &branches_url,
            "--tags",
            &tags_url,
            "--prefix",
            "origin/",
            "--username",
            "alice",
            "--no-auth-cache",
        ])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    let before = git
        .run_for_test(["rev-parse", "refs/remotes/origin/trunk"])
        .unwrap();
    fixture
        .modify_run_script_content("#!/bin/sh\necho askpass\n")
        .unwrap();
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .env("GIT_ASKPASS", &askpass)
        .env("GIT_SVN_RS_ASKPASS_LOG", &askpass_log)
        .args(["fetch", "--no-auth-cache"])
        .assert()
        .success();

    let after = git
        .run_for_test(["rev-parse", "refs/remotes/origin/trunk"])
        .unwrap();
    assert_ne!(after, before);
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:run.sh"])
            .unwrap(),
        "#!/bin/sh\necho askpass\n"
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
fn clone_stdlayout_svn_url_imports_branch_tag_and_copy_contents() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }
    match require_svnserve() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let server = SvnServe::start(fixture.root()).unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &server.repo_url(), "work", "--stdlayout"])
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
    assert_copy_parent_matches_trunk_revision(&git, &work, 1);
}

#[test]
fn fetch_stdlayout_svn_url_imports_trunk_history_after_init() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }
    match require_svnserve() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let server = SvnServe::start(fixture.root()).unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["init", &server.repo_url(), "work", "--stdlayout"])
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
fn fetch_stdlayout_svn_url_imports_branch_tag_and_copy_contents_after_init() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }
    match require_svnserve() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let server = SvnServe::start(fixture.root()).unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["init", &server.repo_url(), "work", "--stdlayout"])
        .assert()
        .success();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
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
        git.run_for_test(["rev-parse", "refs/remotes/origin/main^"])
            .unwrap()
            .trim(),
        trunk_revision_commit(&work, 1).as_str()
    );
}

#[test]
fn clone_file_url_no_metadata_omits_git_svn_id_footer() {
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
            "--no-metadata",
        ])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    let commit = git
        .run_for_test(["show", "-s", "--format=%B", "refs/remotes/origin/trunk"])
        .unwrap();
    assert!(commit.contains("add trunk file"));
    assert!(!commit.contains("git-svn-id: "));
}

#[test]
fn clone_file_url_rewrites_git_svn_id_metadata() {
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
    let rewritten_uuid = "00000000-1111-2222-3333-444444444444";

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "clone",
            &fixture.url(),
            "work",
            "--stdlayout",
            "--rewrite-root",
            "https://mirror.example/svn/project",
            "--rewrite-uuid",
            rewritten_uuid,
        ])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    let commit = git
        .run_for_test(["show", "-s", "--format=%B", "refs/remotes/origin/trunk"])
        .unwrap();
    assert!(commit.contains(
        "git-svn-id: https://mirror.example/svn/project/trunk@2 00000000-1111-2222-3333-444444444444"
    ));
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
        git.run_for_test(["rev-parse", "refs/remotes/origin/main^"])
            .unwrap()
            .trim(),
        trunk_revision_commit(&work, 1).as_str()
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
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/main:empty-dir/.gitkeep"])
            .unwrap(),
        String::new()
    );
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/tags/v1:empty-dir/.gitkeep"])
            .unwrap(),
        String::new()
    );
}

#[test]
fn clone_stdlayout_svn_url_preserves_empty_dirs_when_requested() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }
    match require_svnserve() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let server = SvnServe::start(fixture.root()).unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "clone",
            &server.repo_url(),
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
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/main:empty-dir/.gitkeep"])
            .unwrap(),
        String::new()
    );
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/tags/v1:empty-dir/.gitkeep"])
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
fn incremental_fetch_reconciles_empty_directory_placeholders_from_the_final_tree() {
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
    let upstream = temp.path().join("upstream");
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
    run_svn(temp.path(), &["checkout", &fixture.url(), "upstream"]);

    std::fs::write(upstream.join("trunk/empty-dir/value.txt"), "value\n").unwrap();
    run_svn(
        &upstream,
        &["add", "--non-interactive", "trunk/empty-dir/value.txt"],
    );
    run_svn(
        &upstream,
        &[
            "commit",
            "--non-interactive",
            "-m",
            "fill formerly empty directory",
        ],
    );
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:empty-dir/.gitkeep"])
            .is_err()
    );
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:empty-dir/value.txt"])
            .unwrap(),
        "value\n"
    );

    run_svn(
        &upstream,
        &["delete", "--non-interactive", "trunk/empty-dir/value.txt"],
    );
    run_svn(
        &upstream,
        &["commit", "--non-interactive", "-m", "empty directory again"],
    );
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
        .assert()
        .success();

    assert!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:empty-dir/value.txt"])
            .is_err()
    );
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:empty-dir/.gitkeep"])
            .unwrap(),
        String::new()
    );
    let unhandled =
        std::fs::read_to_string(work.join(".git/svn/refs/remotes/origin/trunk/unhandled.log"))
            .unwrap();
    assert_eq!(
        unhandled
            .lines()
            .filter(|line| *line == "  +empty_dir: trunk/empty-dir")
            .count(),
        2
    );
    assert_eq!(
        unhandled
            .lines()
            .filter(|line| *line == "  -empty_dir: trunk/empty-dir")
            .count(),
        1
    );
}

#[test]
fn incremental_fetch_preserves_binary_unknown_property_bytes_in_unhandled_log() {
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
    let upstream = temp.path().join("upstream");
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    run_svn(temp.path(), &["checkout", &fixture.url(), "upstream"]);

    let property_value = temp.path().join("binary-property-value");
    std::fs::write(
        &property_value,
        [0x00, 0xf0, 0x28, 0x8c, 0x28, 0xff, b'\n', b'%', b'A', b' '],
    )
    .unwrap();
    run_svn(
        &upstream,
        &[
            "propset",
            "--non-interactive",
            "git-svn-rs:binary",
            "--file",
            property_value.to_str().unwrap(),
            "trunk/run.sh",
        ],
    );
    run_svn(
        &upstream,
        &[
            "commit",
            "--non-interactive",
            "-m",
            "set binary custom property",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
        .assert()
        .success();

    let unhandled =
        std::fs::read_to_string(work.join(".git/svn/refs/remotes/origin/trunk/unhandled.log"))
            .unwrap();
    let encoded_property =
        "  +file_prop: trunk/run.sh git-svn-rs:binary %00%F0%28%8C%28%FF%0A%25A%20";
    assert_eq!(
        unhandled
            .lines()
            .filter(|line| *line == encoded_property)
            .count(),
        1,
        "binary property should be recorded byte-exactly:\n{unhandled}"
    );
}

#[test]
fn incremental_fetch_preserves_an_unowned_real_placeholder_named_file() {
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
    let upstream = temp.path().join("upstream");
    run_svn(temp.path(), &["checkout", &fixture.url(), "upstream"]);
    std::fs::create_dir(upstream.join("trunk/real-placeholder")).unwrap();
    std::fs::write(
        upstream.join("trunk/real-placeholder/.gitkeep"),
        "SVN-owned content\n",
    )
    .unwrap();
    run_svn(
        &upstream,
        &["add", "--non-interactive", "trunk/real-placeholder"],
    );
    run_svn(
        &upstream,
        &[
            "commit",
            "--non-interactive",
            "-m",
            "add real placeholder-named file",
        ],
    );

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

    std::fs::write(upstream.join("trunk/real-placeholder/value.txt"), "value\n").unwrap();
    run_svn(
        &upstream,
        &[
            "add",
            "--non-interactive",
            "trunk/real-placeholder/value.txt",
        ],
    );
    run_svn(
        &upstream,
        &[
            "commit",
            "--non-interactive",
            "-m",
            "add sibling to real placeholder-named file",
        ],
    );
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
            "refs/remotes/origin/trunk:real-placeholder/.gitkeep",
        ])
        .unwrap(),
        "SVN-owned content\n"
    );
    assert_eq!(
        git.run_for_test([
            "show",
            "refs/remotes/origin/trunk:real-placeholder/value.txt",
        ])
        .unwrap(),
        "value\n"
    );
    let unhandled =
        std::fs::read_to_string(work.join(".git/svn/refs/remotes/origin/trunk/unhandled.log"))
            .unwrap();
    assert!(!unhandled.contains("empty_dir: trunk/real-placeholder"));
}

#[test]
fn incremental_fetch_restores_placeholder_ownership_from_gc_archive() {
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
    let upstream = temp.path().join("upstream");
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
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("gc")
        .assert()
        .success();
    assert!(
        work.join(".git/svn/refs/remotes/origin/trunk/unhandled.log.gz")
            .exists()
    );

    run_svn(temp.path(), &["checkout", &fixture.url(), "upstream"]);
    std::fs::write(upstream.join("trunk/empty-dir/value.txt"), "value\n").unwrap();
    run_svn(
        &upstream,
        &["add", "--non-interactive", "trunk/empty-dir/value.txt"],
    );
    run_svn(
        &upstream,
        &[
            "commit",
            "--non-interactive",
            "-m",
            "fill empty directory after gc",
        ],
    );
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:empty-dir/.gitkeep"])
            .is_err()
    );
    let unhandled =
        std::fs::read_to_string(work.join(".git/svn/refs/remotes/origin/trunk/unhandled.log"))
            .unwrap();
    assert!(unhandled.contains("  -empty_dir: trunk/empty-dir"));
}

#[test]
fn fetch_parent_updates_only_the_current_first_parent_tracking_identity() {
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
    let upstream = temp.path().join("upstream");
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", &fixture.url(), "work", "--stdlayout"])
        .assert()
        .success();
    let git = git_svn_rs_core::git::GitCli::new(&work);
    let branch_before = git
        .run_for_test(["rev-parse", "refs/remotes/origin/main"])
        .unwrap();
    run_git(
        &work,
        &["checkout", "-b", "topic", "refs/remotes/origin/trunk"],
    );

    run_svn(temp.path(), &["checkout", &fixture.url(), "upstream"]);
    std::fs::write(
        upstream.join("trunk/src/lib.rs"),
        "pub fn trunk_parent_fetch() {}\n",
    )
    .unwrap();
    std::fs::write(
        upstream.join("branches/main/src/lib.rs"),
        "pub fn branch_must_wait() {}\n",
    )
    .unwrap();
    run_svn(
        &upstream,
        &[
            "commit",
            "--non-interactive",
            "-m",
            "change trunk and branch",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["fetch", "--parent"])
        .assert()
        .success();

    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:src/lib.rs"])
            .unwrap(),
        "pub fn trunk_parent_fetch() {}\n"
    );
    assert_eq!(
        git.run_for_test(["rev-parse", "refs/remotes/origin/main"])
            .unwrap(),
        branch_before
    );
}

#[test]
fn fetch_svn_url_preserves_empty_dirs_from_persisted_config() {
    match require_svn_tools() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }
    match require_svnserve() {
        Ok(()) => {}
        Err(SvnToolPolicy::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(SvnToolPolicy::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::tempdir().unwrap();
    let fixture = StandardSvnFixture::create().unwrap();
    let server = SvnServe::start(fixture.root()).unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "init",
            &server.repo_url(),
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
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/main:empty-dir/.gitkeep"])
            .unwrap(),
        String::new()
    );
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/tags/v1:empty-dir/.gitkeep"])
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
fn clone_file_url_applies_ignore_refs_filter() {
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
            "--ignore-refs",
            "^refs/remotes/origin/main$",
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
        refs.lines()
            .any(|line| line == "refs/remotes/origin/tags/v1")
    );
}

#[test]
fn clone_path_filter_keeps_copy_from_excluded_source() {
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
    let upstream = temp.path().join("upstream");
    run_svn(temp.path(), &["checkout", &fixture.url(), "upstream"]);
    std::fs::create_dir_all(upstream.join("trunk/secret")).unwrap();
    std::fs::create_dir_all(upstream.join("trunk/public")).unwrap();
    std::fs::write(
        upstream.join("trunk/secret/template.txt"),
        "copied through filter\n",
    )
    .unwrap();
    run_svn(
        &upstream,
        &["add", "--non-interactive", "trunk/secret", "trunk/public"],
    );
    run_svn(
        &upstream,
        &[
            "commit",
            "--non-interactive",
            "-m",
            "add excluded copy source",
        ],
    );
    run_svn(
        &upstream,
        &[
            "copy",
            "--non-interactive",
            "trunk/secret/template.txt",
            "trunk/public/template.txt",
        ],
    );
    run_svn(
        &upstream,
        &[
            "commit",
            "--non-interactive",
            "-m",
            "copy into included path",
        ],
    );

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
            "^trunk/secret(?:/|$)",
        ])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:public/template.txt"])
            .unwrap(),
        "copied through filter\n"
    );
    assert!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:secret/template.txt"])
            .is_err()
    );
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
            "authors.txt",
        ])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert_eq!(
        git.config_get("svn-remote.svn.authors-file")
            .unwrap()
            .as_deref(),
        authors.to_str()
    );
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

    let fetched_revision = fixture
        .modify_run_script_content("#!/bin/sh\necho authors fetch\n")
        .unwrap();
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
        .assert()
        .success();

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
    assert!(
        git.run_for_test(["show", "-s", "--format=%B", "refs/remotes/origin/trunk"])
            .unwrap()
            .contains(&format!("@{fetched_revision} "))
    );
}

#[test]
fn clone_file_url_applies_authors_prog_mapping() {
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
    let authors_prog = write_authors_prog(temp.path());

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
        "Program Author <program@example.com>"
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

#[test]
fn fetch_file_url_applies_persisted_authors_prog_mapping() {
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
    let authors_prog = write_authors_prog(temp.path());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "init",
            &fixture.url(),
            "work",
            "--stdlayout",
            "--authors-prog",
            authors_prog.to_str().unwrap(),
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
        "Program Author <program@example.com>"
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

fn write_authors_prog(dir: &std::path::Path) -> std::path::PathBuf {
    #[cfg(windows)]
    {
        let path = dir.join("authors-prog.cmd");
        std::fs::write(
            &path,
            "@echo off\r\necho Program Author ^<program@example.com^>\r\n",
        )
        .unwrap();
        path
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;

        let path = dir.join("authors-prog.sh");
        std::fs::write(
            &path,
            "#!/bin/sh\necho 'Program Author <program@example.com>'\n",
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
        path
    }
}

fn trunk_revision_commit(work: &std::path::Path, revision: u32) -> String {
    RevMap::open_existing(trunk_rev_map_path(work), ObjectFormat::Sha1)
        .unwrap()
        .get(revision)
        .unwrap()
        .expect("trunk revision in rev_map")
}

fn trunk_rev_map_path(work: &std::path::Path) -> std::path::PathBuf {
    std::fs::read_dir(work.join(".git/svn/refs/remotes/origin/trunk"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with(".rev_map.")
                        && !name.ends_with(".lock")
                        && !name.ends_with(".lock.tmp")
                })
        })
        .expect("origin/trunk rev_map")
}

fn assert_copy_parent_matches_trunk_revision(
    git: &git_svn_rs_core::git::GitCli,
    work: &std::path::Path,
    revision: u32,
) {
    let actual = git
        .run_for_test(["rev-parse", "refs/remotes/origin/main^"])
        .unwrap();
    let expected = trunk_revision_commit(work, revision);
    assert_eq!(
        actual.trim(),
        expected,
        "branch parent:\n{}\nexpected trunk r{revision}:\n{}",
        git.commit_message(actual.trim()).unwrap(),
        git.commit_message(&expected).unwrap()
    );
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

fn run_svn(cwd: &std::path::Path, args: &[&str]) {
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

fn run_git(cwd: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .current_dir(cwd)
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
