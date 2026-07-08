#[path = "golden/fixtures.rs"]
mod golden_fixtures;

use golden_fixtures::{
    CompatDecision, FileModeArtifact, FilePropertyArtifact, GoldenArtifactCapture,
    GoldenComparisonArtifacts, GoldenFixture, GoldenFixtureStep, RevMapArtifactRecord,
    RevMapByteLengthArtifact, ToolAvailability, compare_supported_subset,
    missing_perl_git_svn_policy, perl_git_svn_available, require_golden_tools, require_svn_tools,
    run_rust_stdlayout_ref_artifacts, run_standard_trunk_golden_comparison,
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
            source_ref: "refs/remotes/origin/trunk".to_string(),
            uuid: "uuid".to_string(),
            revision: 2,
            has_commit: true,
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
        log_oneline_show_commit: "<commit> | r2 | add files".to_string(),
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
        git_svn_id_footers: vec!["git-svn-id: file:///repo@2 uuid".to_string()],
        rev_map: vec![RevMapArtifactRecord {
            source_ref: "refs/remotes/git-svn".to_string(),
            uuid: "other-uuid".to_string(),
            revision: 2,
            has_commit: false,
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
        log_oneline_show_commit: "<commit> | r2 | add trunk file".to_string(),
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
fn rust_stdlayout_golden_correlates_ref_tips_and_rev_maps() {
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
