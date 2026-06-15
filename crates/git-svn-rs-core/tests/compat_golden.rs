#[path = "golden/fixtures.rs"]
mod golden_fixtures;

use golden_fixtures::{
    CompatDecision, FileModeArtifact, GoldenArtifactCapture, GoldenComparisonArtifacts,
    GoldenFixture, GoldenFixtureStep, RevMapArtifactRecord, ToolAvailability,
    compare_supported_subset, missing_perl_git_svn_policy, perl_git_svn_available,
    require_golden_tools, run_standard_trunk_golden_comparison,
};

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
fn artifact_capture_writes_normalized_text_files() {
    let tmp = tempfile::tempdir().unwrap();
    let capture = GoldenArtifactCapture::new(tmp.path(), "case-one").unwrap();

    let artifact = capture
        .write_text("perl/git-log.txt", "line one\r\nline two")
        .unwrap();

    assert_eq!(
        artifact.strip_prefix(tmp.path()).unwrap(),
        std::path::Path::new("case-one/perl/git-log.txt")
    );
    assert_eq!(
        std::fs::read_to_string(artifact).unwrap(),
        "line one\nline two\n"
    );
}

#[test]
fn artifact_comparison_reports_supported_subset_mismatches() {
    let perl = GoldenComparisonArtifacts {
        config: vec![(
            "svn-remote.svn.fetch".to_string(),
            "trunk:refs/remotes/origin/trunk".to_string(),
        )],
        refs: vec!["refs/remotes/origin/trunk".to_string()],
        git_svn_id_footers: vec!["git-svn-id: file:///repo/trunk@2 uuid".to_string()],
        rev_map: vec![RevMapArtifactRecord {
            revision: 2,
            has_commit: true,
        }],
        file_modes: vec![FileModeArtifact {
            mode: "100755".to_string(),
            path: "run.sh".to_string(),
        }],
        log_oneline: "r2 | add files\n".to_string(),
        find_rev: "abc123\n".to_string(),
        info_url: "file:///repo/trunk\n".to_string(),
        info_summary: "URL: file:///repo/trunk\nRevision: 2".to_string(),
    };
    let rust = GoldenComparisonArtifacts {
        config: vec![(
            "svn-remote.svn.fetch".to_string(),
            ":refs/remotes/git-svn".to_string(),
        )],
        refs: vec!["refs/remotes/git-svn".to_string()],
        git_svn_id_footers: vec!["git-svn-id: file:///repo@2 uuid".to_string()],
        rev_map: vec![RevMapArtifactRecord {
            revision: 2,
            has_commit: false,
        }],
        file_modes: vec![FileModeArtifact {
            mode: "100644".to_string(),
            path: "run.sh".to_string(),
        }],
        log_oneline: "r2 | add trunk file\n".to_string(),
        find_rev: "def456\n".to_string(),
        info_url: "file:///repo\n".to_string(),
        info_summary: "URL: file:///repo\nRevision: 1".to_string(),
    };

    let err = compare_supported_subset(&perl, &rust).unwrap_err();

    assert!(err.contains("config differs"));
    assert!(err.contains("refs differ"));
    assert!(err.contains("git-svn-id footers differ"));
    assert!(err.contains("rev_map records differ"));
    assert!(err.contains("file modes differ"));
    assert!(err.contains("log --oneline differs"));
    assert!(err.contains("find-rev output differs"));
    assert!(err.contains("info --url output differs"));
    assert!(err.contains("info output differs"));
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
