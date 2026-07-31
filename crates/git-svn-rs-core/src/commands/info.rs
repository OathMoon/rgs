use crate::cli::InfoArgs;
use crate::commands::resolver::resolve_tracked_svn;
use crate::git::GitCli;
use crate::rev_map::RevMapRecord;
use md5::{Digest, Md5};
use std::path::{Component, Path};
use std::time::UNIX_EPOCH;

struct PathLastChanged {
    author: String,
    revision: u32,
    date: String,
}

pub fn run(args: InfoArgs) -> Result<String, String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: InfoArgs,
) -> Result<String, String> {
    let tracked = resolve_tracked_svn(work_tree)?;
    let relative_path = args.path.as_deref().map(normalize_info_path).transpose()?;
    let base_url = tracked.config.metadata_url(&tracked.svn_path)?;
    let url = relative_path
        .as_deref()
        .map(|path| add_info_path_to_url(&base_url, path))
        .transpose()?
        .unwrap_or(base_url);
    if let Some(path) = relative_path.as_deref() {
        validate_normal_info_path(&tracked.git, path)?;
    }
    if args.url {
        return Ok(format!("{url}\n"));
    }
    crate::tracking_state::validate_existing_tracking_state(
        &tracked.git,
        &tracked.config,
        &tracked.refname,
        &tracked.svn_path,
        &tracked.uuid,
        &tracked.rev_map_path,
    )?;
    let records = tracked.records()?;
    let first_parent_history = tracked.git.first_parent_history("HEAD")?;
    let svn_record = record_for_first_parent(&records, &first_parent_history).ok_or_else(|| {
        format!(
            "unable to determine an SVN revision from HEAD history for {}",
            tracked.refname
        )
    })?;
    let revision = svn_record.revision.to_string();
    let repository_root = tracked
        .git
        .git_svn_metadata_get(&format!("svn-remote.{}.reposRoot", tracked.config.name))?
        .unwrap_or_else(|| tracked.config.url.clone());
    let metadata_uuid = tracked.config.metadata_uuid(&tracked.uuid)?;

    if let (Some(path_arg), Some(path)) = (args.path.as_deref(), relative_path.as_deref()) {
        let entry = if path.is_empty() {
            None
        } else {
            let entry = tracked
                .git
                .ls_tree_file(&svn_record.object_id_hex, path)
                .map_err(|_| {
                    format!(
                        "info [path] currently supports only paths present in the selected SVN revision: {path}"
                    )
                })?;
            let head_entry = tracked.git.ls_tree_file("HEAD", path)?;
            if entry.mode != head_entry.mode {
                return Err(format!(
                    "info [path] currently supports only paths whose node kind matches the selected SVN revision: {path}"
                ));
            }
            Some(entry)
        };
        let node_kind = if entry.as_ref().is_none_or(|entry| entry.mode == "040000") {
            "directory"
        } else {
            "file"
        };
        let name = (node_kind == "file")
            .then(|| Path::new(path).file_name().and_then(|name| name.to_str()))
            .flatten()
            .map(|name| format!("Name: {name}\n"))
            .unwrap_or_default();
        let last_changed = path_last_changed(
            &tracked.git,
            &records,
            &svn_record.object_id_hex,
            path,
            &tracked.config.metadata_url(&tracked.svn_path)?,
            metadata_uuid,
        )?;
        let file_details = entry
            .as_ref()
            .filter(|entry| entry.mode != "040000")
            .map(|entry| file_info_details(&tracked.git, path, &entry.mode))
            .transpose()?
            .unwrap_or_default();
        return Ok(format!(
            "Path: {path_arg}\n{name}URL: {url}\nRepository Root: {repository_root}\nRepository UUID: {}\nRevision: {revision}\nNode Kind: {node_kind}\nSchedule: normal\nLast Changed Author: {}\nLast Changed Rev: {}\nLast Changed Date: {}\n{file_details}\n",
            metadata_uuid, last_changed.author, last_changed.revision, last_changed.date
        ));
    }

    Ok(format!(
        "URL: {url}\nRepository Root: {repository_root}\nRepository UUID: {}\nRevision: {revision}\n",
        metadata_uuid
    ))
}

fn path_last_changed(
    git: &GitCli,
    records: &[RevMapRecord],
    selected_commit: &str,
    path: &str,
    expected_url: &str,
    expected_uuid: &str,
) -> Result<PathLastChanged, String> {
    let pathspec = if path.is_empty() { "." } else { path };
    let raw = git.log_records(
        selected_commit,
        Some(1),
        true,
        false,
        &["--".to_string(), pathspec.to_string()],
    )?;
    let record = raw.trim_start_matches(['\r', '\n', '\x1e']);
    let record = record
        .split_once('\x1d')
        .map_or(record, |(record, _)| record);
    let fields = record.splitn(7, '\x1f').collect::<Vec<_>>();
    if fields.len() != 7 {
        return Err(format!(
            "unable to determine last changed Git record for info path {pathspec}"
        ));
    }
    let revision = records
        .iter()
        .find(|record| record.object_id_hex == fields[0])
        .map(|record| record.revision)
        .ok_or_else(|| {
            format!(
                "last changed commit for info path {pathspec} is absent from the selected rev_map: {}",
                fields[0]
            )
        })?;
    let (footer, _) = super::log::split_git_svn_footer(fields[6]).ok_or_else(|| {
        format!("last changed commit for info path {pathspec} has no valid git-svn-id footer")
    })?;
    if footer.revision != revision || footer.url != expected_url || footer.uuid != expected_uuid {
        return Err(format!(
            "last changed commit for info path {pathspec} disagrees with its rev_map identity"
        ));
    }
    Ok(PathLastChanged {
        author: fields[2].to_string(),
        revision,
        date: super::log::format_svn_date(fields[4], super::log::author_timezone(fields[5])?)?,
    })
}

fn file_info_details(git: &GitCli, path: &str, mode: &str) -> Result<String, String> {
    let modified = std::fs::metadata(git.work_tree().join(path))
        .map_err(|error| format!("failed to stat info path {path}: {error}"))?
        .modified()
        .map_err(|error| format!("failed to read modification time for {path}: {error}"))?;
    let epoch = match modified.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs())
            .map_err(|_| format!("file modification time is out of range: {path}"))?,
        Err(error) => -i64::try_from(error.duration().as_secs())
            .map_err(|_| format!("file modification time is out of range: {path}"))?,
    }
    .to_string();
    let text_last_updated = super::log::format_svn_date(&epoch, "+0000")?;
    let content = if mode == "120000" {
        let mut content = b"link ".to_vec();
        content.extend(git.show_file("HEAD", path)?);
        content
    } else {
        std::fs::read(git.work_tree().join(path))
            .map_err(|error| format!("failed to read info path {path}: {error}"))?
    };
    let checksum = hex::encode(Md5::digest(content));
    Ok(format!(
        "Text Last Updated: {text_last_updated}\nChecksum: {checksum}\n"
    ))
}

fn normalize_info_path(path: &str) -> Result<String, String> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err("info path must be repository-relative and may not contain '..'".to_string());
    }
    Ok(crate::path_url::canonicalize_path(
        path.to_str()
            .ok_or_else(|| "info path is not valid UTF-8".to_string())?,
    ))
}

fn validate_normal_info_path(git: &crate::git::GitCli, path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Ok(());
    }
    git.ls_tree_file("HEAD", path)
        .map_err(|_| format!("svn: '{path}' is not under version control"))?;
    if !git.staged_name_status(path)?.is_empty() {
        return Err(format!(
            "info [path] currently supports only paths without staged changes: {path}"
        ));
    }
    let metadata = std::fs::symlink_metadata(git.work_tree().join(path)).map_err(|_| {
        format!("info [path] currently supports only existing normal paths: {path}")
    })?;
    let entry = git.ls_tree_file("HEAD", path)?;
    let actual = metadata.file_type();
    let kind_matches = if entry.mode == "040000" {
        actual.is_dir()
    } else if entry.mode == "120000" {
        actual.is_symlink()
    } else {
        actual.is_file() && !actual.is_symlink()
    };
    if !kind_matches {
        return Err(format!(
            "info [path] cannot report normal schedule because the worktree node kind differs from HEAD: {path}"
        ));
    }
    Ok(())
}

fn add_info_path_to_url(base_url: &str, path: &str) -> Result<String, String> {
    if path.is_empty() {
        return Ok(base_url.to_string());
    }
    let mut url = url::Url::parse(base_url)
        .map_err(|error| format!("invalid tracked SVN URL {base_url}: {error}"))?;
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| format!("tracked SVN URL cannot contain paths: {base_url}"))?;
    segments.pop_if_empty();
    for segment in path.split('/') {
        segments.push(segment);
    }
    drop(segments);
    Ok(url.into())
}

fn record_for_first_parent<'a>(
    records: &'a [RevMapRecord],
    first_parent_history: &[String],
) -> Option<&'a RevMapRecord> {
    first_parent_history.iter().find_map(|commit| {
        records
            .iter()
            .find(|record| record.object_id_hex == *commit)
    })
}

#[cfg(test)]
mod tests {
    use super::{add_info_path_to_url, record_for_first_parent};
    use crate::rev_map::RevMapRecord;

    #[test]
    fn revision_uses_the_nearest_rev_map_record_in_first_parent_history() {
        let records = vec![
            RevMapRecord {
                revision: 1,
                object_id_hex: "first".to_string(),
            },
            RevMapRecord {
                revision: 3,
                object_id_hex: "latest".to_string(),
            },
        ];

        assert_eq!(
            record_for_first_parent(&records, &["local".to_string(), "first".to_string()])
                .map(|record| record.revision),
            Some(1)
        );
    }

    #[test]
    fn info_url_percent_encodes_each_git_path_segment() {
        assert_eq!(
            add_info_path_to_url("mock://repo/trunk", "dir/space #%.txt").unwrap(),
            "mock://repo/trunk/dir/space%20%23%25.txt"
        );
    }
}
