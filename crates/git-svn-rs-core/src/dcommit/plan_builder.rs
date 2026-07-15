use std::collections::BTreeSet;

use crate::dcommit::diff_planner::{
    ChangeMetadata, DcommitPlan, DcommitTarget, PlannedChange, PropertyChange,
    normalize_commit_path,
};
use crate::git::{GitRawDiffEntry, GitRawDiffStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DcommitPlanRequest {
    pub target: DcommitTarget,
    pub base_revision: u32,
    pub git_commit: String,
    pub message: String,
    pub author: Option<String>,
    pub mergeinfo: Option<String>,
    pub changes: Vec<GitRawDiffEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct DcommitPlanBuilder;

impl DcommitPlanBuilder {
    pub fn new() -> Self {
        Self
    }

    pub fn build(
        &self,
        request: DcommitPlanRequest,
        mut read_file: impl FnMut(&str) -> Result<Vec<u8>, String>,
    ) -> Result<DcommitPlan, String> {
        let mut ensured_dirs = BTreeSet::new();
        let mut parent_dirs = Vec::new();
        let mut copies_and_moves = Vec::new();
        let mut file_changes = Vec::new();
        let mut deletes = Vec::new();

        for raw in request.changes {
            validate_raw_modes(&raw)?;
            let metadata = ChangeMetadata {
                old_mode: raw.old_mode.clone(),
                new_mode: raw.new_mode.clone(),
                old_oid: raw.old_oid.clone(),
                new_oid: raw.new_oid.clone(),
                similarity: raw.similarity,
            };
            match raw.status {
                GitRawDiffStatus::Added => {
                    let path = target_path(&raw)?;
                    ensure_parent_dirs(&path, &mut ensured_dirs, &mut parent_dirs);
                    let content = svn_content(&raw.new_mode, read_file(&path)?);
                    file_changes.push(
                        PlannedChange::add_file(path, content)
                            .with_metadata(metadata)
                            .with_mode_properties(&raw.old_mode, &raw.new_mode),
                    );
                }
                GitRawDiffStatus::Modified | GitRawDiffStatus::TypeChanged => {
                    let path = target_path(&raw)?;
                    let content = svn_content(&raw.new_mode, read_file(&path)?);
                    file_changes.push(
                        PlannedChange::modify_file(path, content)
                            .with_metadata(metadata)
                            .with_mode_properties(&raw.old_mode, &raw.new_mode),
                    );
                }
                GitRawDiffStatus::Deleted => {
                    deletes.push(PlannedChange::delete(source_path(&raw)?).with_metadata(metadata));
                }
                GitRawDiffStatus::Renamed | GitRawDiffStatus::Copied => {
                    let source = source_path(&raw)?;
                    let target = target_path(&raw)?;
                    ensure_parent_dirs(&target, &mut ensured_dirs, &mut parent_dirs);
                    let content = svn_content(&raw.new_mode, read_file(&target)?);
                    let change = if raw.status == GitRawDiffStatus::Renamed {
                        PlannedChange::move_entry(source, request.base_revision, target, content)
                    } else {
                        PlannedChange::copy_file(source, request.base_revision, target, content)
                    };
                    copies_and_moves.push(
                        change
                            .with_metadata(metadata)
                            .with_mode_properties(&raw.old_mode, &raw.new_mode),
                    );
                }
            }
        }

        deletes.sort_by(|left, right| {
            right
                .path
                .matches('/')
                .count()
                .cmp(&left.path.matches('/').count())
                .then_with(|| left.path.cmp(&right.path))
        });
        let mut changes = parent_dirs;
        changes.extend(copies_and_moves);
        changes.extend(file_changes);
        changes.extend(deletes);

        let root_properties = request
            .mergeinfo
            .map(|value| vec![PropertyChange::set("svn:mergeinfo", value)])
            .unwrap_or_default();
        Ok(DcommitPlan {
            target: request.target,
            base_revision: request.base_revision,
            git_commit: request.git_commit,
            message: request.message,
            author: request.author,
            root_properties,
            changes,
        })
    }
}

trait PlannedChangeModeProperties {
    fn with_mode_properties(self, old_mode: &str, new_mode: &str) -> Self;
}

impl PlannedChangeModeProperties for PlannedChange {
    fn with_mode_properties(mut self, old_mode: &str, new_mode: &str) -> Self {
        self.executable = new_mode == "100755";
        self.symlink = new_mode == "120000";
        push_boolean_property_transition(
            &mut self.properties,
            "svn:executable",
            old_mode == "100755",
            self.executable,
        );
        push_boolean_property_transition(
            &mut self.properties,
            "svn:special",
            old_mode == "120000",
            self.symlink,
        );
        self
    }
}

fn push_boolean_property_transition(
    properties: &mut Vec<PropertyChange>,
    name: &str,
    old_value: bool,
    new_value: bool,
) {
    match (old_value, new_value) {
        (false, true) => properties.push(PropertyChange::set(name, "*")),
        (true, false) => properties.push(PropertyChange::delete(name)),
        _ => {}
    }
}

fn svn_content(mode: &str, content: Vec<u8>) -> Vec<u8> {
    if mode == "120000" {
        let mut encoded = b"link ".to_vec();
        encoded.extend(content);
        encoded
    } else {
        content
    }
}

fn validate_raw_modes(change: &GitRawDiffEntry) -> Result<(), String> {
    for (label, mode) in [("old", &change.old_mode), ("new", &change.new_mode)] {
        if !matches!(mode.as_str(), "000000" | "100644" | "100755" | "120000") {
            return Err(format!("unsupported {label} Git mode {mode}"));
        }
    }
    Ok(())
}

fn source_path(change: &GitRawDiffEntry) -> Result<String, String> {
    change
        .source_path
        .as_deref()
        .ok_or_else(|| "raw diff change is missing its source path".to_string())
        .and_then(normalize_commit_path)
}

fn target_path(change: &GitRawDiffEntry) -> Result<String, String> {
    change
        .target_path
        .as_deref()
        .ok_or_else(|| "raw diff change is missing its target path".to_string())
        .and_then(normalize_commit_path)
}

fn ensure_parent_dirs(
    path: &str,
    ensured: &mut BTreeSet<String>,
    changes: &mut Vec<PlannedChange>,
) {
    let components = path.split('/').collect::<Vec<_>>();
    let mut current = String::new();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(component);
        if ensured.insert(current.clone()) {
            changes.push(PlannedChange::ensure_dir(current.clone()));
        }
    }
}
