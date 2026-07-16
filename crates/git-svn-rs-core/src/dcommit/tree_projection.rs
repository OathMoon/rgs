use std::collections::BTreeMap;

use crate::git::GitTreeFile;

use super::diff_planner::{DcommitPlan, PlannedChange, PlannedChangeKind};

pub fn apply_plan_to_tree(tree: &mut BTreeMap<String, GitTreeFile>, plan: &DcommitPlan) {
    for change in &plan.changes {
        match change.kind {
            PlannedChangeKind::EnsureDir => {}
            PlannedChangeKind::Delete => remove_path(tree, &change.path),
            PlannedChangeKind::Move => {
                if let Some(source) = &change.source {
                    remove_path(tree, &source.path);
                }
                insert_file(tree, change);
            }
            PlannedChangeKind::AddFile
            | PlannedChangeKind::ModifyFile
            | PlannedChangeKind::CopyFile => insert_file(tree, change),
        }
    }
    canonicalize_tree_keywords(tree, plan);
}

pub fn canonicalize_tree_keywords(tree: &mut BTreeMap<String, GitTreeFile>, plan: &DcommitPlan) {
    for change in &plan.changes {
        let Some(keywords) = change
            .properties
            .iter()
            .rev()
            .find(|property| property.name == "svn:keywords")
            .and_then(|property| property.value.as_deref())
        else {
            continue;
        };
        let Some(file) = tree.get_mut(&change.path) else {
            continue;
        };
        for keyword in keywords.split_whitespace() {
            file.content = contract_keyword(&file.content, keyword);
        }
    }
}

pub fn tree_map(files: Vec<GitTreeFile>) -> BTreeMap<String, GitTreeFile> {
    files
        .into_iter()
        .map(|file| (file.path.clone(), file))
        .collect()
}

fn remove_path(tree: &mut BTreeMap<String, GitTreeFile>, path: &str) {
    let prefix = format!("{path}/");
    tree.retain(|candidate, _| candidate != path && !candidate.starts_with(&prefix));
}

fn insert_file(tree: &mut BTreeMap<String, GitTreeFile>, change: &PlannedChange) {
    let Some(mut content) = change.content.clone() else {
        return;
    };
    let mut special = change.symlink;
    let mut executable = change.executable;
    let mut eol_style = None;
    for property in &change.properties {
        match property.name.as_str() {
            "svn:special" => special = property.value.is_some(),
            "svn:executable" => executable = property.value.is_some(),
            "svn:eol-style" => eol_style = property.value.as_deref(),
            _ => {}
        }
    }
    if let Some(style) = eol_style {
        content = normalize_eol(content, style);
    }
    let mode = if special {
        if content.starts_with(b"link ") {
            content.drain(..5);
        }
        "120000"
    } else if executable {
        "100755"
    } else {
        "100644"
    };
    tree.insert(
        change.path.clone(),
        GitTreeFile {
            path: change.path.clone(),
            mode: mode.to_string(),
            content,
        },
    );
}

fn normalize_eol(content: Vec<u8>, style: &str) -> Vec<u8> {
    let mut lf = Vec::with_capacity(content.len());
    let mut index = 0;
    while index < content.len() {
        if content[index] == b'\r' {
            if content.get(index + 1) == Some(&b'\n') {
                index += 1;
            }
            lf.push(b'\n');
        } else {
            lf.push(content[index]);
        }
        index += 1;
    }
    let separator: &[u8] = match style.to_ascii_uppercase().as_str() {
        "CRLF" => b"\r\n",
        "CR" => b"\r",
        "NATIVE" if cfg!(windows) => b"\r\n",
        _ => return lf,
    };
    let mut normalized = Vec::with_capacity(lf.len());
    for byte in lf {
        if byte == b'\n' {
            normalized.extend_from_slice(separator);
        } else {
            normalized.push(byte);
        }
    }
    normalized
}

fn contract_keyword(content: &[u8], keyword: &str) -> Vec<u8> {
    let marker = format!("${keyword}:").into_bytes();
    let replacement = format!("${keyword}$").into_bytes();
    let mut contracted = Vec::with_capacity(content.len());
    let mut offset = 0;
    while let Some(relative) = content[offset..]
        .windows(marker.len())
        .position(|window| window == marker)
    {
        let start = offset + relative;
        contracted.extend_from_slice(&content[offset..start]);
        let Some(end) = content[start + marker.len()..]
            .iter()
            .position(|byte| *byte == b'$')
            .map(|relative| start + marker.len() + relative)
        else {
            contracted.extend_from_slice(&content[start..]);
            return contracted;
        };
        contracted.extend_from_slice(&replacement);
        offset = end + 1;
    }
    contracted.extend_from_slice(&content[offset..]);
    contracted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcommit::diff_planner::{DcommitTarget, PlannedChange, PropertyChange};

    fn plan(changes: Vec<PlannedChange>) -> DcommitPlan {
        DcommitPlan {
            target: DcommitTarget {
                url: "file:///repo/trunk".to_string(),
                repository_root: "file:///repo".to_string(),
                repository_uuid: "uuid".to_string(),
                git_ref: "refs/remotes/git-svn".to_string(),
            },
            base_revision: 1,
            git_commit: "commit".to_string(),
            message: "message".to_string(),
            author: None,
            root_properties: Vec::new(),
            changes,
        }
    }

    #[test]
    fn properties_project_git_modes_and_symlink_content() {
        let mut tree = BTreeMap::new();
        let changes = vec![
            PlannedChange::add_file("executable", b"run\n")
                .with_property(PropertyChange::set("svn:executable", "custom")),
            PlannedChange::add_file("link", b"link target")
                .with_property(PropertyChange::set("svn:special", "custom")),
        ];

        apply_plan_to_tree(&mut tree, &plan(changes));

        assert_eq!(tree["executable"].mode, "100755");
        assert_eq!(tree["link"].mode, "120000");
        assert_eq!(tree["link"].content, b"target");
    }

    #[test]
    fn later_property_deletes_override_mode_flags() {
        let mut tree = BTreeMap::new();
        let changes = vec![
            PlannedChange::add_file("regular", b"content")
                .with_executable(true)
                .with_symlink(true)
                .with_property(PropertyChange::delete("svn:executable"))
                .with_property(PropertyChange::delete("svn:special")),
        ];

        apply_plan_to_tree(&mut tree, &plan(changes));

        assert_eq!(tree["regular"].mode, "100644");
        assert_eq!(tree["regular"].content, b"content");
    }

    #[test]
    fn move_and_directory_delete_update_projected_paths() {
        let mut tree = tree_map(vec![
            GitTreeFile {
                path: "old".to_string(),
                mode: "100644".to_string(),
                content: b"old".to_vec(),
            },
            GitTreeFile {
                path: "dir/child".to_string(),
                mode: "100644".to_string(),
                content: b"child".to_vec(),
            },
        ]);
        let changes = vec![
            PlannedChange::move_entry("old", 1, "new", b"new"),
            PlannedChange::delete("dir"),
        ];

        apply_plan_to_tree(&mut tree, &plan(changes));

        assert_eq!(tree.keys().cloned().collect::<Vec<_>>(), vec!["new"]);
        assert_eq!(tree["new"].content, b"new");
    }

    #[test]
    fn eol_style_projects_translated_import_content() {
        let mut tree = BTreeMap::new();
        let changes = vec![
            PlannedChange::add_file("text", b"one\ntwo\n")
                .with_property(PropertyChange::set("svn:eol-style", "CRLF")),
        ];

        apply_plan_to_tree(&mut tree, &plan(changes));

        assert_eq!(tree["text"].content, b"one\r\ntwo\r\n");
    }

    #[test]
    fn keyword_expansions_are_compared_in_canonical_form() {
        let mut tree = tree_map(vec![GitTreeFile {
            path: "version.rs".to_string(),
            mode: "100644".to_string(),
            content: b"const ID: &str = \"$Id: version.rs 5 2026-01-01 user $\";\n".to_vec(),
        }]);
        let changes = vec![
            PlannedChange::modify_file("version.rs", b"$Id$")
                .with_property(PropertyChange::set("svn:keywords", "Id")),
        ];

        canonicalize_tree_keywords(&mut tree, &plan(changes));

        assert_eq!(tree["version.rs"].content, b"const ID: &str = \"$Id$\";\n");
    }
}
