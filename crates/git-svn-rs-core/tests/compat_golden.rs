#[path = "golden/fixtures.rs"]
mod golden_fixtures;
#[allow(dead_code)]
#[path = "support/svn_fixture.rs"]
mod svn_fixture;

use git_svn_rs_core::cli::{FetchArgs, InitArgs, LayoutArgs, SharedFetchArgs};
use git_svn_rs_core::commands;
use git_svn_rs_core::git::GitCli;
use golden_fixtures::{
    CloneStateArtifact, CommitGraphArtifact, CompatDecision, FileModeArtifact,
    FilePropertyArtifact, GoldenArtifactCapture, GoldenComparisonArtifacts, GoldenFixture,
    GoldenFixtureStep, RefTipArtifact, RevMapArtifactRecord, RevMapByteLengthArtifact,
    ToolAvailability, compare_supported_subset, missing_perl_git_svn_policy,
    perl_git_svn_available, require_golden_tools, require_svn_tools,
    run_rust_stdlayout_ref_artifacts, run_standard_full_url_layout_golden_comparison,
    run_standard_stdlayout_golden_comparison, run_standard_subdirectory_golden_comparison,
    run_standard_trunk_golden_comparison, supported_rev_map, supported_rev_map_byte_lengths,
};
#[cfg(unix)]
use golden_fixtures::{
    require_golden_svnserve, run_standard_authenticated_svn_dcommit_golden_comparison,
    run_standard_dcommit_golden_comparison, run_standard_dirty_dcommit_golden_comparison,
    run_standard_recovery_dcommit_golden_comparison,
};
use std::path::Path;
use std::process::Command;

#[test]
fn missing_perl_git_svn_skips_by_default_and_fails_in_strict_mode() {
    assert_eq!(
        missing_perl_git_svn_policy(false),
        CompatDecision::Skip("skipping: Perl git-svn is required".to_string())
    );
    assert_eq!(
        missing_perl_git_svn_policy(true),
        CompatDecision::Fail("Perl git-svn is required".to_string())
    );
}

#[test]
fn perl_git_svn_detector_reports_available_or_missing_without_panicking() {
    match perl_git_svn_available() {
        ToolAvailability::Available { version } => assert!(version.contains("git-svn")),
        ToolAvailability::Missing { reason } => assert!(!reason.trim().is_empty()),
    }
}

#[test]
fn standard_fixture_manifest_is_deterministic() {
    let fixture = GoldenFixture::standard_linear_history();

    assert_eq!(fixture.name(), "standard-linear-history");
    assert_eq!(
        fixture.steps(),
        &[
            GoldenFixtureStep::CreateStandardLayout,
            GoldenFixtureStep::AddFile {
                path: "trunk/src/lib.rs",
                contents: "pub fn answer() -> u8 { 42 }\n",
            },
            GoldenFixtureStep::AddFile {
                path: "trunk/run.sh",
                contents: "#!/bin/sh\necho hi\n",
            },
            GoldenFixtureStep::AddFile {
                path: "trunk/link-to-lib",
                contents: "link src/lib.rs",
            },
            GoldenFixtureStep::AddFile {
                path: "trunk/deleted.txt",
                contents: "temporary\n",
            },
            GoldenFixtureStep::SetProperty {
                path: "trunk/run.sh",
                name: "svn:executable",
                value: "x",
            },
            GoldenFixtureStep::SetProperty {
                path: "trunk/link-to-lib",
                name: "svn:special",
                value: "x",
            },
            GoldenFixtureStep::SetProperty {
                path: "trunk/src/lib.rs",
                name: "svn:eol-style",
                value: "LF",
            },
            GoldenFixtureStep::SetProperty {
                path: "trunk/src/lib.rs",
                name: "svn:mime-type",
                value: "text/plain",
            },
            GoldenFixtureStep::SetProperty {
                path: "trunk/src/lib.rs",
                name: "svn:keywords",
                value: "Id",
            },
            GoldenFixtureStep::SetProperty {
                path: "trunk/src/lib.rs",
                name: "svn:needs-lock",
                value: "x",
            },
            GoldenFixtureStep::AddEmptyDir {
                path: "trunk/empty-dir",
            },
            GoldenFixtureStep::Copy {
                from: "trunk",
                to: "branches/main",
            },
            GoldenFixtureStep::Copy {
                from: "trunk",
                to: "tags/v1",
            },
            GoldenFixtureStep::Delete {
                path: "trunk/deleted.txt",
            },
        ]
    );
}

#[test]
fn standard_fixture_manifest_records_svn_properties() {
    let fixture = GoldenFixture::standard_linear_history();

    assert!(fixture.steps().contains(&GoldenFixtureStep::SetProperty {
        path: "trunk/run.sh",
        name: "svn:executable",
        value: "x",
    }));
    assert!(fixture.steps().contains(&GoldenFixtureStep::SetProperty {
        path: "trunk/link-to-lib",
        name: "svn:special",
        value: "x",
    }));
    assert!(fixture.steps().contains(&GoldenFixtureStep::SetProperty {
        path: "trunk/src/lib.rs",
        name: "svn:eol-style",
        value: "LF",
    }));
    assert!(fixture.steps().contains(&GoldenFixtureStep::SetProperty {
        path: "trunk/src/lib.rs",
        name: "svn:mime-type",
        value: "text/plain",
    }));
    assert!(fixture.steps().contains(&GoldenFixtureStep::SetProperty {
        path: "trunk/src/lib.rs",
        name: "svn:keywords",
        value: "Id",
    }));
    assert!(fixture.steps().contains(&GoldenFixtureStep::SetProperty {
        path: "trunk/src/lib.rs",
        name: "svn:needs-lock",
        value: "x",
    }));
}

#[test]
fn artifact_capture_writes_normalized_text_files() {
    let tmp = tempfile::tempdir().unwrap();
    let capture_root = std::env::var_os("GIT_SVN_RS_COMPAT_ARTIFACT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| tmp.path().to_path_buf());
    let capture = GoldenArtifactCapture::new(tmp.path(), "case-one").unwrap();

    let artifact = capture
        .write_text("perl/git-log.txt", "line one\r\nline two")
        .unwrap();

    assert_eq!(
        artifact.strip_prefix(&capture_root).unwrap(),
        std::path::Path::new("case-one/perl/git-log.txt")
    );
    assert_eq!(
        std::fs::read_to_string(artifact).unwrap(),
        "line one\nline two\n"
    );
    let summary =
        std::fs::read_to_string(capture_root.join("case-one/scenario-summary.json")).unwrap();
    assert!(summary.contains("\"scenario\": \"case-one\""));
    assert!(summary.contains("\"status\": \"started\""));
    assert!(summary.contains("\"frozen_git_commit\": \"0b13e48"));
}

#[test]
fn artifact_comparison_reports_supported_subset_mismatches() {
    let perl = GoldenComparisonArtifacts {
        config: vec![(
            "svn-remote.svn.fetch".to_string(),
            "trunk:refs/remotes/origin/trunk".to_string(),
        )],
        refs: vec!["refs/remotes/origin/trunk".to_string()],
        ref_tips: vec![RefTipArtifact {
            name: "refs/remotes/origin/trunk".to_string(),
            object_id: "11".repeat(20),
        }],
        commit_graph: vec![CommitGraphArtifact {
            object_id: "11".repeat(20),
            parents: Vec::new(),
            tree_id: "22".repeat(20),
            author_name: "Alice".to_string(),
            author_email: "alice@example.com".to_string(),
            author_epoch: 1_700_000_000,
            author_offset: "+0000".to_string(),
            committer_name: "Alice".to_string(),
            committer_email: "alice@example.com".to_string(),
            committer_epoch: 1_700_000_000,
            committer_offset: "+0000".to_string(),
            message: "add files\n".to_string(),
        }],
        clone_state: CloneStateArtifact {
            head_symbolic_ref: Some("refs/heads/master".to_string()),
            head_object_id: Some("11".repeat(20)),
            local_branches: vec![format!("refs/heads/master\t{}\t", "11".repeat(20))],
            index_entries: vec![format!("100644 {} 0\tfile", "22".repeat(20))],
            worktree_entries: vec!["file\t666f6f".to_string()],
            status_porcelain_v2: "# branch.head master\n".to_string(),
        },
        no_checkout_clone_state: CloneStateArtifact {
            head_symbolic_ref: Some("refs/heads/master".to_string()),
            head_object_id: Some("11".repeat(20)),
            ..CloneStateArtifact::default()
        },
        git_svn_id_footers: vec!["git-svn-id: file:///repo/trunk@2 uuid".to_string()],
        rev_map: vec![RevMapArtifactRecord {
            source_ref: "refs/remotes/origin/trunk".to_string(),
            uuid: "uuid".to_string(),
            revision: 2,
            object_id: Some("11".repeat(20)),
        }],
        rev_map_byte_lengths: vec![RevMapByteLengthArtifact {
            source_ref: "refs/remotes/origin/trunk".to_string(),
            uuid: "uuid".to_string(),
            byte_len: 24,
        }],
        file_modes: vec![FileModeArtifact {
            mode: "100755".to_string(),
            path: "run.sh".to_string(),
        }],
        file_properties: vec![FilePropertyArtifact {
            path: "run.sh".to_string(),
            name: "svn:executable".to_string(),
            value: "*".to_string(),
        }],
        tree_contents: vec!["run.sh\t#!/bin/sh\\necho hi\\n".to_string()],
        empty_dir_placeholders: vec!["empty-dir/.gitkeep".to_string()],
        log_oneline: "r2 | add files\n".to_string(),
        log_incremental: "r2 | add files".to_string(),
        log_verbose: "revision r2\npath A src/lib.rs\nsubject add files".to_string(),
        log_oneline_show_commit: "r2 | <commit> | add files".to_string(),
        log_limit_oneline: "r2 | add files".to_string(),
        log_path_oneline: "r2 | add files".to_string(),
        find_rev: "abc123\n".to_string(),
        info_url: "file:///repo/trunk\n".to_string(),
        info_summary: "URL: file:///repo/trunk\nRevision: 2".to_string(),
        log_revision_oneline: "r2 | add files".to_string(),
        log_revision_range_oneline: "r2 | add files\nr1 | layout".to_string(),
        log_revision_reverse_range_oneline: "r1 | layout\nr2 | add files".to_string(),
        find_rev_nearest: "before r3 -> <commit>\nafter r3 -> <commit>".to_string(),
        find_rev_commit: "<commit> -> r2".to_string(),
        rebase_dry_run: "would fetch\nwould rebase <ref>".to_string(),
        reset: "reset r2\nr3 -> ".to_string(),
        gc_output: "gc: success".to_string(),
        clone_output: "Initialized empty Git repository\n".to_string(),
    };
    let rust = GoldenComparisonArtifacts {
        config: vec![(
            "svn-remote.svn.fetch".to_string(),
            ":refs/remotes/git-svn".to_string(),
        )],
        refs: vec!["refs/remotes/git-svn".to_string()],
        ref_tips: vec![RefTipArtifact {
            name: "refs/remotes/git-svn".to_string(),
            object_id: "33".repeat(20),
        }],
        commit_graph: vec![CommitGraphArtifact {
            object_id: "33".repeat(20),
            parents: vec!["44".repeat(20)],
            tree_id: "55".repeat(20),
            author_name: "Bob".to_string(),
            author_email: "bob@example.com".to_string(),
            author_epoch: 1_800_000_000,
            author_offset: "+0800".to_string(),
            committer_name: "Bob".to_string(),
            committer_email: "bob@example.com".to_string(),
            committer_epoch: 1_800_000_000,
            committer_offset: "+0800".to_string(),
            message: "different\n".to_string(),
        }],
        clone_state: CloneStateArtifact::default(),
        no_checkout_clone_state: CloneStateArtifact::default(),
        git_svn_id_footers: vec!["git-svn-id: file:///repo@2 uuid".to_string()],
        rev_map: vec![RevMapArtifactRecord {
            source_ref: "refs/remotes/git-svn".to_string(),
            uuid: "other-uuid".to_string(),
            revision: 2,
            object_id: None,
        }],
        rev_map_byte_lengths: vec![RevMapByteLengthArtifact {
            source_ref: "refs/remotes/git-svn".to_string(),
            uuid: "other-uuid".to_string(),
            byte_len: 48,
        }],
        file_modes: vec![FileModeArtifact {
            mode: "100644".to_string(),
            path: "run.sh".to_string(),
        }],
        file_properties: vec![FilePropertyArtifact {
            path: "run.sh".to_string(),
            name: "svn:eol-style".to_string(),
            value: "LF".to_string(),
        }],
        tree_contents: vec!["run.sh\t#!/bin/sh\\necho bye\\n".to_string()],
        empty_dir_placeholders: Vec::new(),
        log_oneline: "r2 | add trunk file\n".to_string(),
        log_incremental: "r2 | add trunk file".to_string(),
        log_verbose: "revision r2\npath M src/lib.rs\nsubject add trunk file".to_string(),
        log_oneline_show_commit: "r2 | <commit> | add trunk file".to_string(),
        log_limit_oneline: "r3 | branch main".to_string(),
        log_path_oneline: "r3 | branch main".to_string(),
        find_rev: "def456\n".to_string(),
        info_url: "file:///repo\n".to_string(),
        info_summary: "URL: file:///repo\nRevision: 1".to_string(),
        log_revision_oneline: "r3 | branch main".to_string(),
        log_revision_range_oneline: "r3 | branch main\nr2 | add trunk file".to_string(),
        log_revision_reverse_range_oneline: "r2 | add trunk file\nr3 | branch main".to_string(),
        find_rev_nearest: "before r3 -> <commit>\nafter r3 -> ".to_string(),
        find_rev_commit: "<commit> -> r3".to_string(),
        rebase_dry_run: "would fetch\nwould rebase refs/remotes/git-svn".to_string(),
        reset: "reset r1\nr2 -> ".to_string(),
        gc_output: "gc: unsupported".to_string(),
        clone_output: "unsupported output\n".to_string(),
    };

    let err = compare_supported_subset(&perl, &rust).unwrap_err();

    assert!(err.contains("config differs"));
    assert!(err.contains("refs differ"));
    assert!(err.contains("ref tips differ"));
    assert!(err.contains("commit graph differs"));
    assert!(err.contains("clone state differs"));
    assert!(err.contains("--no-checkout clone state differs"));
    assert!(err.contains("git-svn-id footers differ"));
    assert!(err.contains("rev_map records differ"));
    assert!(err.contains("rev_map byte lengths differ"));
    assert!(err.contains("file modes differ"));
    assert!(err.contains("file properties differ"));
    assert!(err.contains("tree contents differ"));
    assert!(err.contains("empty dir placeholders differ"));
    assert!(err.contains("log --oneline differs"));
    assert!(err.contains("log --incremental differs"));
    assert!(err.contains("log --verbose differs"));
    assert!(err.contains("log --oneline --show-commit differs"));
    assert!(err.contains("log --limit output differs"));
    assert!(err.contains("log pathspec output differs"));
    assert!(err.contains("find-rev output differs"));
    assert!(err.contains("info --url output differs"));
    assert!(err.contains("info output differs"));
    assert!(err.contains("log --revision output differs"));
    assert!(err.contains("log --revision range output differs"));
    assert!(err.contains("log --revision reverse range output differs"));
    assert!(err.contains("find-rev nearest output differs"));
    assert!(err.contains("find-rev commit output differs"));
    assert!(err.contains("rebase --dry-run output differs"));
    assert!(err.contains("reset output differs"));
    assert!(err.contains("gc output differs"));
    assert!(err.contains("clone output differs"));
}

#[test]
fn standard_trunk_fixture_matches_perl_git_svn_supported_subset() {
    match require_golden_tools() {
        Ok(version) => {
            eprintln!("Perl git-svn available ({version}); running golden comparison");
        }
        Err(CompatDecision::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(CompatDecision::Fail(message)) => panic!("{message}"),
    }

    let tmp = tempfile::Builder::new()
        .prefix("golden-compat-")
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let comparison = run_standard_trunk_golden_comparison(tmp.path()).unwrap();

    comparison.assert_supported_subset_matches().unwrap();
}

#[test]
fn standard_layout_fixture_matches_perl_git_svn_supported_subset() {
    if cfg!(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked)) {
        eprintln!("stdlayout golden comparison is covered by the SVN CLI compatibility backend");
        return;
    }
    match require_golden_tools() {
        Ok(version) => {
            eprintln!("Perl git-svn available ({version}); running stdlayout golden comparison");
        }
        Err(CompatDecision::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(CompatDecision::Fail(message)) => panic!("{message}"),
    }

    let tmp = tempfile::Builder::new()
        .prefix("golden-compat-stdlayout-")
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let comparison = run_standard_stdlayout_golden_comparison(tmp.path()).unwrap();

    comparison.assert_supported_subset_matches().unwrap();
}

#[test]
fn full_url_layout_fixture_matches_perl_git_svn_supported_subset() {
    if cfg!(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked)) {
        eprintln!(
            "full URL layout golden comparison is covered by the SVN CLI compatibility backend"
        );
        return;
    }
    match require_golden_tools() {
        Ok(version) => {
            eprintln!(
                "Perl git-svn available ({version}); running full URL layout golden comparison"
            );
        }
        Err(CompatDecision::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(CompatDecision::Fail(message)) => panic!("{message}"),
    }

    let tmp = tempfile::Builder::new()
        .prefix("golden-compat-full-url-layout-")
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let comparison = run_standard_full_url_layout_golden_comparison(tmp.path()).unwrap();

    comparison.assert_supported_subset_matches().unwrap();
}

#[test]
fn single_subdirectory_fixture_matches_perl_git_svn_supported_subset() {
    if cfg!(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked)) {
        eprintln!(
            "single-subdirectory golden comparison is covered by the SVN CLI compatibility backend"
        );
        return;
    }
    match require_golden_tools() {
        Ok(version) => {
            eprintln!(
                "Perl git-svn available ({version}); running single-subdirectory golden comparison"
            );
        }
        Err(CompatDecision::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(CompatDecision::Fail(message)) => panic!("{message}"),
    }

    let tmp = tempfile::Builder::new()
        .prefix("golden-compat-subdirectory-")
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let comparison = run_standard_subdirectory_golden_comparison(tmp.path()).unwrap();

    comparison.assert_supported_subset_matches().unwrap();
}

#[cfg(unix)]
#[test]
fn linear_dcommit_write_artifacts_match_frozen_perl_git_svn() {
    if cfg!(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked)) {
        eprintln!("dcommit golden comparison is covered by the SVN CLI compatibility backend");
        return;
    }
    match require_golden_tools() {
        Ok(version) => {
            eprintln!("Perl git-svn available ({version}); running dcommit golden comparison");
        }
        Err(golden_fixtures::CompatDecision::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(golden_fixtures::CompatDecision::Fail(message)) => panic!("{message}"),
    }

    let tmp = tempfile::Builder::new()
        .prefix("golden-compat-dcommit-")
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let comparison = run_standard_dcommit_golden_comparison(tmp.path()).unwrap();
    comparison.assert_write_artifacts_match().unwrap();
}

#[cfg(unix)]
#[test]
fn authenticated_svn_dcommit_write_artifacts_match_frozen_perl_git_svn() {
    if cfg!(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked)) {
        eprintln!(
            "authenticated dcommit golden comparison is covered by the SVN CLI compatibility backend"
        );
        return;
    }
    match require_golden_tools().and_then(|version| {
        require_golden_svnserve()?;
        Ok(version)
    }) {
        Ok(version) => {
            eprintln!(
                "Perl git-svn available ({version}); running authenticated dcommit golden comparison"
            );
        }
        Err(golden_fixtures::CompatDecision::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(golden_fixtures::CompatDecision::Fail(message)) => panic!("{message}"),
    }

    let tmp = tempfile::Builder::new()
        .prefix("golden-compat-auth-dcommit-")
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let comparison = run_standard_authenticated_svn_dcommit_golden_comparison(tmp.path()).unwrap();
    comparison.assert_write_artifacts_match().unwrap();
}

#[cfg(unix)]
#[test]
fn recovered_dcommit_write_artifacts_match_frozen_perl_git_svn() {
    if cfg!(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked)) {
        eprintln!(
            "recovery dcommit golden comparison is covered by the SVN CLI compatibility backend"
        );
        return;
    }
    match require_golden_tools() {
        Ok(version) => {
            eprintln!(
                "Perl git-svn available ({version}); running recovery dcommit golden comparison"
            );
        }
        Err(golden_fixtures::CompatDecision::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(golden_fixtures::CompatDecision::Fail(message)) => panic!("{message}"),
    }

    let tmp = tempfile::Builder::new()
        .prefix("golden-compat-recovery-dcommit-")
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let comparison = run_standard_recovery_dcommit_golden_comparison(tmp.path()).unwrap();
    comparison.assert_write_artifacts_match().unwrap();
}

#[cfg(unix)]
#[test]
fn dirty_dcommit_no_write_artifacts_match_frozen_perl_git_svn() {
    if cfg!(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked)) {
        eprintln!(
            "dirty dcommit golden comparison is covered by the SVN CLI compatibility backend"
        );
        return;
    }
    match require_golden_tools() {
        Ok(version) => {
            eprintln!(
                "Perl git-svn available ({version}); running dirty dcommit no-write comparison"
            );
        }
        Err(golden_fixtures::CompatDecision::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(golden_fixtures::CompatDecision::Fail(message)) => panic!("{message}"),
    }

    let tmp = tempfile::Builder::new()
        .prefix("golden-compat-dirty-dcommit-")
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let comparison = run_standard_dirty_dcommit_golden_comparison(tmp.path()).unwrap();
    comparison.assert_write_artifacts_match().unwrap();
}

#[test]
fn auxiliary_follow_parent_ref_matches_frozen_perl_git_svn() {
    if cfg!(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked)) {
        eprintln!("auxiliary golden comparison is covered by the SVN CLI compatibility backend");
        return;
    }
    match require_golden_tools() {
        Ok(version) => {
            eprintln!("Perl git-svn available ({version}); comparing auxiliary follow-parent");
        }
        Err(CompatDecision::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(CompatDecision::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::Builder::new()
        .prefix("golden-follow-parent-")
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let repo = temp.path().join("repo");
    let upstream = temp.path().join("upstream");
    run_command(temp.path(), "svnadmin", &["create", path_text(&repo)]);
    let url = format!("file://{}", repo.display());
    run_command(
        temp.path(),
        "svn",
        &["checkout", "--non-interactive", &url, path_text(&upstream)],
    );
    std::fs::create_dir_all(upstream.join("trunk")).unwrap();
    std::fs::write(upstream.join("trunk/readme"), "hello\n").unwrap();
    run_command(&upstream, "svn", &["add", "--non-interactive", "trunk"]);
    run_command(
        &upstream,
        "svn",
        &["commit", "--non-interactive", "-m", "initial"],
    );
    std::fs::write(upstream.join("trunk/readme"), "hello\nworld\n").unwrap();
    run_command(
        &upstream,
        "svn",
        &["commit", "--non-interactive", "-m", "another commit"],
    );
    run_command(&upstream, "svn", &["update", "--non-interactive"]);
    run_command(
        &upstream,
        "svn",
        &["mv", "--non-interactive", "trunk", "thunk"],
    );
    run_command(
        &upstream,
        "svn",
        &["commit", "--non-interactive", "-m", "move trunk"],
    );
    run_command(&upstream, "svn", &["update", "--non-interactive"]);
    run_command(
        &upstream,
        "svn",
        &["mv", "--non-interactive", "thunk", "thonk"],
    );
    run_command(
        &upstream,
        "svn",
        &["commit", "--non-interactive", "-m", "move trunk again"],
    );

    let perl = temp.path().join("perl");
    let thonk_url = format!("{}/thonk", url.trim_end_matches('/'));
    run_command(
        temp.path(),
        "git",
        &[
            "svn",
            "init",
            "--minimize-url",
            "--ignore-refs",
            "(?:@|thonk$)",
            "-i",
            "origin/thonk",
            &thonk_url,
            path_text(&perl),
        ],
    );
    run_command(&perl, "git", &["svn", "fetch", "-r", "1:4"]);

    let rust = temp.path().join("rust");
    let mut rust_shared = shared_fetch_args(None);
    rust_shared.ignore_refs = Some("(?:@|thonk$)".to_string());
    commands::init::run(InitArgs {
        url,
        path: Some(path_text(&rust).to_string()),
        layout: LayoutArgs {
            stdlayout: false,
            trunk: Some("thonk".to_string()),
            branches: Vec::new(),
            tags: Vec::new(),
            prefix: None,
        },
        shared: rust_shared,
    })
    .unwrap();
    run_command(
        &rust,
        "git",
        &["config", "--unset-all", "svn-remote.svn.fetch"],
    );
    GitCli::new(&rust)
        .config_add("svn-remote.svn.fetch", "thonk:refs/remotes/origin/thonk")
        .unwrap();
    commands::fetch::run_in_work_tree(
        &rust,
        FetchArgs {
            remote: None,
            shared: shared_fetch_args(Some("1:4")),
            fetch_all: false,
            parent: false,
        },
    )
    .unwrap();

    let perl_git = GitCli::new(&perl);
    let rust_git = GitCli::new(&rust);
    let mut perl_fetch = perl_git.config_get_all("svn-remote.svn.fetch").unwrap();
    let mut rust_fetch = rust_git.config_get_all("svn-remote.svn.fetch").unwrap();
    perl_fetch.sort();
    rust_fetch.sort();
    assert_eq!(
        rust_fetch,
        perl_fetch,
        "svn config differs\nperl:\n{}\nrust:\n{}",
        svn_config(&perl),
        svn_config(&rust)
    );
    assert_eq!(
        ref_tips(&rust),
        ref_tips(&perl),
        "auxiliary and destination commit identities must match frozen Perl\nperl:\n{}\nrust:\n{}",
        history_details(&perl, "refs/remotes/origin/thonk"),
        history_details(&rust, "refs/remotes/origin/thonk")
    );
}

#[test]
fn ancestor_directory_copy_discovery_matches_frozen_perl_git_svn() {
    if cfg!(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked)) {
        eprintln!(
            "ancestor-copy golden comparison is covered by the SVN CLI compatibility backend"
        );
        return;
    }
    match require_golden_tools() {
        Ok(version) => {
            eprintln!("Perl git-svn available ({version}); comparing ancestor-copy discovery");
        }
        Err(CompatDecision::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(CompatDecision::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::Builder::new()
        .prefix("golden-ancestor-copy-")
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let repo = temp.path().join("repo");
    let upstream = temp.path().join("upstream");
    run_command(temp.path(), "svnadmin", &["create", path_text(&repo)]);
    let url = format!("file://{}", repo.display());
    run_command(
        temp.path(),
        "svn",
        &["checkout", "--non-interactive", &url, path_text(&upstream)],
    );
    std::fs::create_dir_all(upstream.join("trunk")).unwrap();
    std::fs::create_dir_all(upstream.join("archive/promoted/a")).unwrap();
    std::fs::write(upstream.join("trunk/readme"), "trunk\n").unwrap();
    std::fs::write(
        upstream.join("archive/promoted/a/file.txt"),
        "ancestor copy\n",
    )
    .unwrap();
    run_command(
        &upstream,
        "svn",
        &["add", "--non-interactive", "trunk", "archive"],
    );
    run_command(
        &upstream,
        "svn",
        &[
            "commit",
            "--non-interactive",
            "-m",
            "create archived branch layout",
        ],
    );
    run_command(
        &upstream,
        "svn",
        &["copy", "--non-interactive", "archive/promoted", "promoted"],
    );
    run_command(
        &upstream,
        "svn",
        &[
            "commit",
            "--non-interactive",
            "-m",
            "promote archived branch layout",
        ],
    );

    let perl = temp.path().join("perl");
    run_command(
        temp.path(),
        "git",
        &[
            "svn",
            "init",
            "--trunk=trunk",
            "--branches=promoted/*",
            "--prefix=origin/",
            &url,
            path_text(&perl),
        ],
    );
    run_command(&perl, "git", &["svn", "fetch"]);

    let rust = temp.path().join("rust");
    commands::init::run(InitArgs {
        url,
        path: Some(path_text(&rust).to_string()),
        layout: LayoutArgs {
            stdlayout: false,
            trunk: Some("trunk".to_string()),
            branches: vec!["promoted/*".to_string()],
            tags: Vec::new(),
            prefix: Some("origin/".to_string()),
        },
        shared: shared_fetch_args(None),
    })
    .unwrap();
    commands::fetch::run_in_work_tree(
        &rust,
        FetchArgs {
            remote: None,
            shared: shared_fetch_args(None),
            fetch_all: false,
            parent: false,
        },
    )
    .unwrap();

    assert_eq!(
        ref_tips(&rust),
        ref_tips(&perl),
        "ancestor-copy refs differ\nperl:\n{}\nrust:\n{}",
        history_details(&perl, "refs/remotes/origin/a"),
        history_details(&rust, "refs/remotes/origin/a")
    );
}

#[test]
fn sparse_fixed_mapping_scan_marker_matches_frozen_perl_git_svn() {
    if cfg!(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked)) {
        eprintln!("scan-marker golden comparison is covered by the SVN CLI compatibility backend");
        return;
    }
    match require_golden_tools() {
        Ok(version) => {
            eprintln!("Perl git-svn available ({version}); comparing fixed scan marker");
        }
        Err(CompatDecision::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(CompatDecision::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::Builder::new()
        .prefix("golden-scan-marker-")
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let repo = temp.path().join("repo");
    let upstream = temp.path().join("upstream");
    run_command(temp.path(), "svnadmin", &["create", path_text(&repo)]);
    let url = format!("file://{}", repo.display());
    run_command(
        temp.path(),
        "svn",
        &["checkout", "--non-interactive", &url, path_text(&upstream)],
    );
    std::fs::create_dir_all(upstream.join("trunk")).unwrap();
    std::fs::create_dir_all(upstream.join("unrelated")).unwrap();
    std::fs::write(upstream.join("trunk/file.txt"), "one\n").unwrap();
    std::fs::write(upstream.join("unrelated/file.txt"), "one\n").unwrap();
    run_command(
        &upstream,
        "svn",
        &["add", "--non-interactive", "trunk", "unrelated"],
    );
    run_command(
        &upstream,
        "svn",
        &["commit", "--non-interactive", "-m", "initial"],
    );
    std::fs::write(upstream.join("unrelated/file.txt"), "two\n").unwrap();
    run_command(
        &upstream,
        "svn",
        &["commit", "--non-interactive", "-m", "unrelated two"],
    );
    std::fs::write(upstream.join("unrelated/file.txt"), "three\n").unwrap();
    run_command(
        &upstream,
        "svn",
        &["commit", "--non-interactive", "-m", "unrelated three"],
    );

    let perl = temp.path().join("perl");
    run_command(
        temp.path(),
        "git",
        &[
            "svn",
            "init",
            "--trunk=trunk",
            "--prefix=origin/",
            &url,
            path_text(&perl),
        ],
    );
    run_command(&perl, "git", &["svn", "fetch"]);

    let rust = temp.path().join("rust");
    commands::init::run(InitArgs {
        url,
        path: Some(path_text(&rust).to_string()),
        layout: LayoutArgs {
            stdlayout: false,
            trunk: Some("trunk".to_string()),
            branches: Vec::new(),
            tags: Vec::new(),
            prefix: Some("origin/".to_string()),
        },
        shared: shared_fetch_args(None),
    })
    .unwrap();
    commands::fetch::run_in_work_tree(
        &rust,
        FetchArgs {
            remote: None,
            shared: shared_fetch_args(None),
            fetch_all: false,
            parent: false,
        },
    )
    .unwrap();

    let refs = vec!["refs/remotes/origin/trunk".to_string()];
    let initial_records = supported_rev_map(&perl, &refs).unwrap();
    let initial_lengths = supported_rev_map_byte_lengths(&perl, &refs).unwrap();
    assert_eq!(supported_rev_map(&rust, &refs).unwrap(), initial_records);
    assert_eq!(
        supported_rev_map_byte_lengths(&rust, &refs).unwrap(),
        initial_lengths
    );
    let marker = initial_records.last().unwrap();
    assert_eq!(marker.revision, 3);
    assert_eq!(marker.object_id, None);

    run_command(&perl, "git", &["svn", "fetch"]);
    commands::fetch::run_in_work_tree(
        &rust,
        FetchArgs {
            remote: None,
            shared: shared_fetch_args(None),
            fetch_all: false,
            parent: false,
        },
    )
    .unwrap();
    assert_eq!(supported_rev_map(&rust, &refs).unwrap(), initial_records);
    assert_eq!(
        supported_rev_map_byte_lengths(&rust, &refs).unwrap(),
        initial_lengths
    );

    std::fs::write(upstream.join("trunk/file.txt"), "two\n").unwrap();
    run_command(
        &upstream,
        "svn",
        &["commit", "--non-interactive", "-m", "trunk two"],
    );
    run_command(&perl, "git", &["svn", "fetch"]);
    commands::fetch::run_in_work_tree(
        &rust,
        FetchArgs {
            remote: None,
            shared: shared_fetch_args(None),
            fetch_all: false,
            parent: false,
        },
    )
    .unwrap();

    let final_records = supported_rev_map(&perl, &refs).unwrap();
    assert_eq!(supported_rev_map(&rust, &refs).unwrap(), final_records);
    assert_eq!(
        supported_rev_map_byte_lengths(&rust, &refs).unwrap(),
        supported_rev_map_byte_lengths(&perl, &refs).unwrap()
    );
    assert_eq!(final_records.len(), initial_records.len());
    assert_eq!(final_records.last().unwrap().revision, 4);
    assert!(final_records.last().unwrap().object_id.is_some());
}

#[test]
fn discovery_high_water_matches_frozen_perl_across_incremental_fetch() {
    if cfg!(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked)) {
        eprintln!("discovery golden comparison is covered by the SVN CLI compatibility backend");
        return;
    }
    match require_golden_tools() {
        Ok(version) => {
            eprintln!("Perl git-svn available ({version}); comparing discovery high-water");
        }
        Err(CompatDecision::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(CompatDecision::Fail(message)) => panic!("{message}"),
    }

    let temp = tempfile::Builder::new()
        .prefix("golden-discovery-")
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let repo = temp.path().join("repo");
    let upstream = temp.path().join("upstream");
    run_command(temp.path(), "svnadmin", &["create", path_text(&repo)]);
    let url = format!("file://{}", repo.display());
    run_command(
        temp.path(),
        "svn",
        &["checkout", "--non-interactive", &url, path_text(&upstream)],
    );
    run_command(
        &upstream,
        "svn",
        &["mkdir", "--non-interactive", "trunk", "branches", "tags"],
    );
    std::fs::write(upstream.join("trunk/file.txt"), "one\n").unwrap();
    run_command(
        &upstream,
        "svn",
        &["add", "--non-interactive", "trunk/file.txt"],
    );
    run_command(
        &upstream,
        "svn",
        &["commit", "--non-interactive", "-m", "layout and trunk"],
    );
    run_command(
        &upstream,
        "svn",
        &["copy", "--non-interactive", "trunk", "branches/main"],
    );
    run_command(
        &upstream,
        "svn",
        &["commit", "--non-interactive", "-m", "branch"],
    );
    run_command(&upstream, "svn", &["update", "--non-interactive"]);
    run_command(
        &upstream,
        "svn",
        &["copy", "--non-interactive", "trunk", "tags/v1"],
    );
    run_command(
        &upstream,
        "svn",
        &["commit", "--non-interactive", "-m", "tag"],
    );

    let perl = temp.path().join("perl");
    run_command(
        temp.path(),
        "git",
        &[
            "svn",
            "init",
            "--stdlayout",
            "--prefix=origin/",
            &url,
            path_text(&perl),
        ],
    );
    run_command(&perl, "git", &["svn", "fetch"]);

    let rust = temp.path().join("rust");
    commands::init::run(InitArgs {
        url,
        path: Some(path_text(&rust).to_string()),
        layout: LayoutArgs {
            stdlayout: true,
            trunk: None,
            branches: Vec::new(),
            tags: Vec::new(),
            prefix: None,
        },
        shared: shared_fetch_args(None),
    })
    .unwrap();
    commands::fetch::run_in_work_tree(
        &rust,
        FetchArgs {
            remote: None,
            shared: shared_fetch_args(None),
            fetch_all: false,
            parent: false,
        },
    )
    .unwrap();
    assert_discovery_high_water_matches(&perl, &rust);

    std::fs::write(upstream.join("trunk/file.txt"), "two\n").unwrap();
    run_command(
        &upstream,
        "svn",
        &["commit", "--non-interactive", "-m", "trunk only"],
    );
    run_command(&perl, "git", &["svn", "fetch"]);
    commands::fetch::run_in_work_tree(
        &rust,
        FetchArgs {
            remote: None,
            shared: shared_fetch_args(None),
            fetch_all: false,
            parent: false,
        },
    )
    .unwrap();
    assert_discovery_high_water_matches(&perl, &rust);
}

fn assert_discovery_high_water_matches(perl: &Path, rust: &Path) {
    let perl_git = GitCli::new(perl);
    let rust_git = GitCli::new(rust);
    for kind in ["branches", "tags"] {
        let key = format!("svn-remote.svn.{kind}-maxRev");
        let perl_value = perl_git.git_svn_metadata_get(&key).unwrap();
        assert!(perl_value.is_some(), "Perl did not persist {key}");
        assert_eq!(rust_git.git_svn_metadata_get(&key).unwrap(), perl_value);
    }
}

fn shared_fetch_args(revision: Option<&str>) -> SharedFetchArgs {
    SharedFetchArgs {
        authors_file: None,
        authors_prog: None,
        ignore_paths: None,
        include_paths: None,
        ignore_refs: None,
        revision: revision.map(str::to_string),
        log_window_size: None,
        localtime: false,
        no_metadata: false,
        use_svnsync_props: false,
        rewrite_root: None,
        rewrite_uuid: None,
        username: None,
        password: None,
        config_dir: None,
        no_auth_cache: false,
        preserve_empty_dirs: false,
        placeholder_filename: ".gitignore".to_string(),
    }
}

fn path_text(path: &Path) -> &str {
    path.to_str().expect("test paths are UTF-8")
}

fn run_command(cwd: &Path, program: &str, args: &[&str]) {
    let output = Command::new(program)
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{program} {args:?} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn ref_tips(work_tree: &Path) -> Vec<String> {
    let output = Command::new("git")
        .current_dir(work_tree)
        .args([
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            "refs/remotes/origin",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

fn history_details(work_tree: &Path, refname: &str) -> String {
    let output = Command::new("git")
        .current_dir(work_tree)
        .args([
            "log",
            "--format=commit %H%nparents %P%nauthor %an <%ae> %at %ai%ncommitter %cn <%ce> %ct %ci%n%B%x00",
            "--reverse",
            refname,
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn svn_config(work_tree: &Path) -> String {
    let output = Command::new("git")
        .current_dir(work_tree)
        .args(["config", "--get-regexp", "^svn-remote\\."])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn rust_stdlayout_golden_correlates_ref_tips_and_rev_maps() {
    if cfg!(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked)) {
        eprintln!("stdlayout golden comparison is covered by the SVN CLI compatibility backend");
        return;
    }
    match require_svn_tools() {
        Ok(()) => {}
        Err(CompatDecision::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(CompatDecision::Fail(message)) => panic!("{message}"),
    }

    let tmp = tempfile::Builder::new()
        .prefix("golden-stdlayout-")
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let artifacts = run_rust_stdlayout_ref_artifacts(tmp.path()).unwrap();

    let expected = [
        ("refs/remotes/origin/main", "/branches/main", 4),
        ("refs/remotes/origin/tags/v1", "/tags/v1", 5),
        ("refs/remotes/origin/trunk", "/trunk", 6),
    ];
    assert_eq!(artifacts.len(), expected.len());
    for (artifact, (source_ref, url_suffix, revision)) in artifacts.iter().zip(expected) {
        assert_eq!(artifact.source_ref, source_ref);
        assert!(artifact.url.ends_with(url_suffix), "{}", artifact.url);
        assert_eq!(artifact.revision, revision);
        assert_eq!(artifact.max_valid_rev_map_revision, revision);
        assert!(
            artifact
                .tree_contents
                .iter()
                .any(|entry| entry == "src/lib.rs\tpub fn answer() -> u8 { 42 }\\n"),
            "{source_ref} tree contents: {:?}",
            artifact.tree_contents
        );
    }
    let main = artifacts
        .iter()
        .find(|artifact| artifact.source_ref == "refs/remotes/origin/main")
        .unwrap();
    let tag = artifacts
        .iter()
        .find(|artifact| artifact.source_ref == "refs/remotes/origin/tags/v1")
        .unwrap();
    let trunk = artifacts
        .iter()
        .find(|artifact| artifact.source_ref == "refs/remotes/origin/trunk")
        .unwrap();
    assert!(
        main.tree_contents
            .iter()
            .any(|entry| entry == "deleted.txt\ttemporary\\n"),
        "{:?}",
        main.tree_contents
    );
    assert!(
        tag.tree_contents
            .iter()
            .any(|entry| entry == "deleted.txt\ttemporary\\n"),
        "{:?}",
        tag.tree_contents
    );
    assert!(
        !trunk
            .tree_contents
            .iter()
            .any(|entry| entry.starts_with("deleted.txt\t")),
        "{:?}",
        trunk.tree_contents
    );
    assert!(!artifacts[0].uuid.is_empty());
    assert!(
        artifacts
            .iter()
            .all(|artifact| artifact.uuid == artifacts[0].uuid)
    );
}
