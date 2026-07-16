use crate::dcommit::diff_planner::{DcommitPlan, PlannedChangeKind, PropertyChange};

pub fn svn_file_properties(attributes: &str, path: &str) -> Vec<(String, String)> {
    let mut svn_properties = None;
    let mut property_operations = Vec::new();
    let mut attribute_order = 0;
    for line in attributes.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(pattern) = parts.next() else {
            continue;
        };
        if !attribute_pattern_matches(pattern, path) {
            continue;
        }
        for attr in parts {
            attribute_order += 1;
            if let Some(value) = attr.strip_prefix("svn-properties=") {
                svn_properties = Some((attribute_order, value));
            } else if attr == "-svn-properties" || attr == "!svn-properties" {
                svn_properties = None;
            } else if let Some(name) = direct_svn_property_clear(attr) {
                property_operations.push((attribute_order, name, None));
            } else if let Some(value) = attr.strip_prefix("svn:eol-style=") {
                property_operations.push((attribute_order, "svn:eol-style", Some(value)));
            } else if let Some(value) = attr.strip_prefix("svn:mime-type=") {
                property_operations.push((attribute_order, "svn:mime-type", Some(value)));
            } else if let Some(value) = attr.strip_prefix("svn:keywords=") {
                property_operations.push((attribute_order, "svn:keywords", Some(value)));
            } else if let Some(value) = attr.strip_prefix("svn:needs-lock=") {
                property_operations.push((attribute_order, "svn:needs-lock", Some(value)));
            } else if let Some(value) = attr.strip_prefix("svn:executable=") {
                property_operations.push((attribute_order, "svn:executable", Some(value)));
            } else if attr == "svn:executable" {
                property_operations.push((attribute_order, "svn:executable", Some("x")));
            } else if let Some(value) = attr.strip_prefix("svn:special=") {
                property_operations.push((attribute_order, "svn:special", Some(value)));
            } else if attr == "svn:special" {
                property_operations.push((attribute_order, "svn:special", Some("x")));
            } else if attr == "svn:needs-lock" {
                property_operations.push((attribute_order, "svn:needs-lock", Some("x")));
            }
        }
    }
    if let Some((order, value)) = svn_properties {
        for property in value.split(';').filter(|property| !property.is_empty()) {
            if let Some((name, value)) = property.split_once('=')
                && !name.is_empty()
                && !value.is_empty()
            {
                property_operations.push((order, name, Some(value)));
            }
        }
    }
    property_operations.sort_by_key(|(order, _, _)| *order);
    let mut svn_props = Vec::new();
    for (_, name, value) in property_operations {
        apply_svn_file_attribute_operation(&mut svn_props, name, value);
    }
    svn_props
}

pub fn merge_attribute_properties(
    plan: &mut DcommitPlan,
    base_attributes: Option<&str>,
    current_attributes: Option<&str>,
) {
    let base_attributes = base_attributes.unwrap_or_default();
    let current_attributes = current_attributes.unwrap_or_default();

    for change in &mut plan.changes {
        let old_properties = match change.kind {
            PlannedChangeKind::AddFile => Vec::new(),
            PlannedChangeKind::ModifyFile => svn_file_properties(base_attributes, &change.path),
            PlannedChangeKind::CopyFile | PlannedChangeKind::Move => change
                .source
                .as_ref()
                .map(|source| svn_file_properties(base_attributes, &source.path))
                .unwrap_or_default(),
            PlannedChangeKind::EnsureDir | PlannedChangeKind::Delete => continue,
        };
        let new_properties = svn_file_properties(current_attributes, &change.path);
        for (property, _) in &old_properties {
            if new_properties
                .iter()
                .all(|(new_property, _)| new_property != property)
            {
                change
                    .properties
                    .push(PropertyChange::delete(property.clone()));
            }
        }
        for (property, value) in new_properties {
            change.properties.push(PropertyChange::set(property, value));
        }
    }
}

fn direct_svn_property_clear(attr: &str) -> Option<&'static str> {
    match attr {
        "-svn:eol-style" | "!svn:eol-style" => Some("svn:eol-style"),
        "-svn:mime-type" | "!svn:mime-type" => Some("svn:mime-type"),
        "-svn:keywords" | "!svn:keywords" => Some("svn:keywords"),
        "-svn:executable" | "!svn:executable" => Some("svn:executable"),
        "-svn:special" | "!svn:special" => Some("svn:special"),
        "-svn:needs-lock" | "!svn:needs-lock" => Some("svn:needs-lock"),
        _ => None,
    }
}

fn apply_svn_file_attribute_operation(
    props: &mut Vec<(String, String)>,
    name: &str,
    value: Option<&str>,
) {
    if let Some(value) = value {
        if let Some((_, existing)) = props.iter_mut().find(|(property, _)| property == name) {
            *existing = value.to_string();
        } else {
            props.push((name.to_string(), value.to_string()));
        }
    } else if let Some(index) = props.iter().position(|(property, _)| property == name) {
        props.remove(index);
    }
}

fn attribute_pattern_matches(pattern: &str, path: &str) -> bool {
    if pattern == path {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return path.ends_with(suffix);
    }
    if let Some((prefix, suffix)) = pattern.split_once('*') {
        let Some(rest) = path.strip_prefix(prefix) else {
            return false;
        };
        return rest.ends_with(suffix) && !rest.trim_end_matches(suffix).contains('/');
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcommit::diff_planner::{DcommitTarget, PlannedChange};

    #[test]
    fn later_matching_attribute_overrides_earlier_value() {
        let properties = svn_file_properties(
            "*.txt svn:eol-style=LF\nnotes.txt svn:eol-style=CRLF\n",
            "notes.txt",
        );

        assert_eq!(
            properties,
            vec![("svn:eol-style".to_string(), "CRLF".to_string())]
        );
    }

    #[test]
    fn direct_clear_removes_an_earlier_property() {
        let properties = svn_file_properties(
            "*.lock svn:needs-lock\nclear.lock -svn:needs-lock\n",
            "clear.lock",
        );

        assert!(properties.is_empty());
    }

    #[test]
    fn malformed_compound_properties_are_filtered_without_reordering_valid_values() {
        let properties = svn_file_properties(
            "*.txt svn-properties=svn:eol-style=LF;svn:keywords=Id;;missing;=ignored;svn:mime-type=; svn:eol-style=CRLF\n",
            "notes.txt",
        );

        assert_eq!(
            properties,
            vec![
                ("svn:eol-style".to_string(), "CRLF".to_string()),
                ("svn:keywords".to_string(), "Id".to_string()),
            ]
        );
    }

    #[test]
    fn rename_compares_base_source_path_with_current_target_path() {
        let mut plan = plan_with_change(PlannedChange::move_entry("old.txt", 7, "new.txt", b"new"));

        merge_attribute_properties(
            &mut plan,
            Some("old.txt svn:eol-style=LF svn:keywords=Id\n"),
            Some("new.txt svn:eol-style=CRLF\n"),
        );

        assert_eq!(
            plan.changes[0].properties,
            vec![
                PropertyChange::delete("svn:keywords"),
                PropertyChange::set("svn:eol-style", "CRLF"),
            ]
        );
    }

    fn plan_with_change(change: PlannedChange) -> DcommitPlan {
        DcommitPlan {
            target: DcommitTarget {
                url: "file:///repo/trunk".to_string(),
                repository_root: "file:///repo".to_string(),
                repository_uuid: "uuid".to_string(),
                git_ref: "refs/remotes/git-svn".to_string(),
            },
            base_revision: 7,
            git_commit: "commit".to_string(),
            message: "message".to_string(),
            author: None,
            root_properties: Vec::new(),
            changes: vec![change],
        }
    }
}
