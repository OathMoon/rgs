use std::collections::BTreeMap;

use git_svn_rs_core::dcommit::{
    DcommitPlanBuilder, DcommitPlanRequest, DcommitTarget, PlannedChangeKind, PropertyChange,
};
use git_svn_rs_core::git::{GitRawDiffEntry, GitRawDiffStatus};

fn target() -> DcommitTarget {
    DcommitTarget {
        url: "file:///repo/trunk".to_string(),
        repository_root: "file:///repo".to_string(),
        repository_uuid: "uuid".to_string(),
        git_ref: "refs/remotes/git-svn".to_string(),
    }
}

fn raw(
    status: GitRawDiffStatus,
    old_mode: &str,
    new_mode: &str,
    source: Option<&str>,
    destination: Option<&str>,
) -> GitRawDiffEntry {
    GitRawDiffEntry {
        old_mode: old_mode.to_string(),
        new_mode: new_mode.to_string(),
        old_oid: "a".repeat(40),
        new_oid: "b".repeat(40),
        status,
        similarity: matches!(status, GitRawDiffStatus::Renamed | GitRawDiffStatus::Copied)
            .then_some(73),
        source_path: source.map(str::to_string),
        target_path: destination.map(str::to_string),
    }
}

#[test]
fn builder_preserves_copy_move_content_metadata_and_order() {
    let request = DcommitPlanRequest {
        target: target(),
        base_revision: 6,
        git_commit: "c".repeat(40),
        message: "subject\n\nbody\n".to_string(),
        author: Some("A U Thor <author@example.com>".to_string()),
        mergeinfo: Some("/branches/topic:1-6".to_string()),
        changes: vec![
            raw(
                GitRawDiffStatus::Modified,
                "100644",
                "100755",
                Some("source.txt"),
                Some("source.txt"),
            ),
            raw(
                GitRawDiffStatus::Copied,
                "100644",
                "100644",
                Some("source.txt"),
                Some("nested/copied.txt"),
            ),
            raw(
                GitRawDiffStatus::Renamed,
                "100644",
                "120000",
                Some("old-link"),
                Some("nested/new-link"),
            ),
            raw(
                GitRawDiffStatus::Deleted,
                "100644",
                "000000",
                Some("deep/old.txt"),
                None,
            ),
        ],
    };
    let files = BTreeMap::from([
        ("source.txt", b"modified\n".to_vec()),
        ("nested/copied.txt", b"copied and changed\n".to_vec()),
        ("nested/new-link", b"target.txt".to_vec()),
    ]);

    let plan = DcommitPlanBuilder::new()
        .build(request, |path| {
            files
                .get(path)
                .cloned()
                .ok_or_else(|| format!("missing {path}"))
        })
        .unwrap();

    assert_eq!(plan.message, "subject\n\nbody\n");
    assert_eq!(
        plan.root_properties,
        vec![PropertyChange::set("svn:mergeinfo", "/branches/topic:1-6")]
    );
    assert_eq!(
        plan.changes
            .iter()
            .map(|change| (&change.kind, change.path.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (&PlannedChangeKind::EnsureDir, "nested"),
            (&PlannedChangeKind::CopyFile, "nested/copied.txt"),
            (&PlannedChangeKind::Move, "nested/new-link"),
            (&PlannedChangeKind::ModifyFile, "source.txt"),
            (&PlannedChangeKind::Delete, "deep/old.txt"),
        ]
    );
    assert_eq!(
        plan.changes[1].content.as_deref(),
        Some(&b"copied and changed\n"[..])
    );
    assert_eq!(
        plan.changes[1].metadata.as_ref().unwrap().similarity,
        Some(73)
    );
    assert_eq!(
        plan.changes[2].content.as_deref(),
        Some(&b"link target.txt"[..])
    );
    assert_eq!(
        plan.changes[2].properties,
        vec![PropertyChange::set("svn:special", "*")]
    );
    assert_eq!(
        plan.changes[3].properties,
        vec![PropertyChange::set("svn:executable", "*")]
    );
}

#[test]
fn builder_emits_property_deletes_for_type_changes_and_rejects_modes() {
    let mut request = DcommitPlanRequest {
        target: target(),
        base_revision: 6,
        git_commit: "c".repeat(40),
        message: "type change\n".to_string(),
        author: None,
        mergeinfo: None,
        changes: vec![raw(
            GitRawDiffStatus::TypeChanged,
            "120000",
            "100644",
            Some("link"),
            Some("link"),
        )],
    };
    let plan = DcommitPlanBuilder::new()
        .build(request.clone(), |_| Ok(b"ordinary\n".to_vec()))
        .unwrap();
    assert_eq!(
        plan.changes[0].properties,
        vec![PropertyChange::delete("svn:special")]
    );

    request.changes[0].new_mode = "160000".to_string();
    let error = DcommitPlanBuilder::new()
        .build(request, |_| Ok(Vec::new()))
        .unwrap_err();
    assert!(error.contains("unsupported new Git mode 160000"));
}
