#[path = "golden/fixtures.rs"]
mod golden_fixtures;

use golden_fixtures::{
    CompatDecision, GoldenArtifactCapture, GoldenFixture, GoldenFixtureStep, ToolAvailability,
    missing_perl_git_svn_policy, perl_git_svn_available, require_perl_git_svn,
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
            GoldenFixtureStep::Copy {
                from: "trunk",
                to: "branches/main",
            },
            GoldenFixtureStep::Copy {
                from: "trunk",
                to: "tags/v1",
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
fn rust_vs_perl_comparison_placeholder_skips_until_fetch_artifacts_exist() {
    match require_perl_git_svn() {
        Ok(version) => {
            eprintln!(
                "Perl git-svn available ({version}); golden comparison awaits production fetch artifacts"
            );
        }
        Err(CompatDecision::Skip(message)) => {
            eprintln!("{message}");
            return;
        }
        Err(CompatDecision::Fail(message)) => panic!("{message}"),
    }

    let fixture = GoldenFixture::standard_linear_history();
    assert_eq!(fixture.name(), "standard-linear-history");
}
