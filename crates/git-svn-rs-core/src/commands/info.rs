use crate::cli::InfoArgs;
use crate::commands::resolver::TrackedSvn;
use crate::commands::resolver::resolve_tracked_svn;
use crate::git::GitCli;
use crate::git_svn_id::GitSvnId;
use crate::rev_map::RevMapRecord;
use crate::tracking_state::{IdentityRevMaps, TrackingIdentityKind};
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
    if tracked.config.no_metadata {
        return Err(
            "info is unavailable for --no-metadata imports because working-tree history has no git-svn-id metadata"
                .to_string(),
        );
    }
    let relative_path = args.path.as_deref().map(normalize_info_path).transpose()?;
    if args.url && !tracked.config.use_svm_props {
        let base_url = tracked.config.metadata_url(&tracked.svn_path)?;
        let url = relative_path
            .as_deref()
            .map(|path| add_info_path_to_url(&base_url, path))
            .transpose()?
            .unwrap_or(base_url);
        if let Some(path) = relative_path.as_deref() {
            validate_normal_info_path(&tracked.git, path)?;
        }
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
    let transport_records = tracked.records()?;
    let first_parent_history = tracked.git.first_parent_history("HEAD")?;
    let transport_record = record_for_first_parent(&transport_records, &first_parent_history)
        .ok_or_else(|| {
            format!(
                "unable to determine an SVN revision from HEAD history for {}",
                tracked.refname
            )
        })?;
    let identity = validated_commit_identity(
        &tracked,
        &transport_records,
        &transport_record.object_id_hex,
    )?;
    let base_url = identity.id.url.clone();
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
    let revision = identity.record.revision.to_string();
    let repository_root = tracked
        .git
        .git_svn_metadata_get(&format!("svn-remote.{}.reposRoot", tracked.config.name))?
        .unwrap_or_else(|| tracked.config.url.clone());
    let metadata_uuid = identity.id.uuid.as_str();

    if let (Some(path_arg), Some(path)) = (args.path.as_deref(), relative_path.as_deref()) {
        let entry = if path.is_empty() {
            None
        } else {
            let entry = tracked
                .git
                .ls_tree_file(&transport_record.object_id_hex, path)
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
            &tracked,
            &transport_records,
            &transport_record.object_id_hex,
            path,
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
    tracked: &TrackedSvn,
    transport_records: &[RevMapRecord],
    selected_commit: &str,
    path: &str,
) -> Result<PathLastChanged, String> {
    let git = &tracked.git;
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
    let identity =
        validated_commit_identity(tracked, transport_records, fields[0]).map_err(|_| {
            format!(
                "last changed commit for info path {pathspec} disagrees with its rev_map identity"
            )
        })?;
    Ok(PathLastChanged {
        author: fields[2].to_string(),
        revision: identity.record.revision,
        date: super::log::format_svn_date(fields[4], super::log::author_timezone(fields[5])?)?,
    })
}

pub(crate) struct ValidatedCommitIdentity {
    pub kind: TrackingIdentityKind,
    pub record: RevMapRecord,
    pub id: GitSvnId,
}

pub(crate) fn validated_commit_identity(
    tracked: &TrackedSvn,
    transport_records: &[RevMapRecord],
    commit: &str,
) -> Result<ValidatedCommitIdentity, String> {
    let transport_record = transport_records
        .iter()
        .find(|record| record.object_id_hex == commit && !record.has_zero_object_id())
        .ok_or_else(|| format!("commit {commit} is absent from the transport rev_map"))?;
    let message = tracked.git.commit_message(commit)?;
    let footer = message
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| format!("commit {commit} has no git-svn-id footer"))?;
    let id = GitSvnId::parse(footer.trim_end_matches('\r'))
        .map_err(|_| format!("commit {commit} has no valid git-svn-id footer"))?;
    let kind = crate::tracking_state::classify_tracking_identity(
        &tracked.git,
        &tracked.config,
        &tracked.svn_path,
        IdentityRevMaps {
            transport_uuid: &tracked.uuid,
            transport_record,
            source_path: tracked
                .source_rev_map
                .as_ref()
                .map(|source| source.path.as_path()),
        },
        Some(&id),
    )?
    .ok_or_else(|| format!("commit {commit} disagrees with its rev_map identity"))?;
    let record = match kind {
        TrackingIdentityKind::Transport => transport_record.clone(),
        TrackingIdentityKind::Source => tracked
            .source_rev_map
            .as_ref()
            .ok_or_else(|| "SVM source rev_map is missing".to_string())
            .and_then(|source| {
                crate::rev_map::RevMap::open_existing(&source.path, tracked.git.object_format()?)
            })?
            .records()?
            .into_iter()
            .find(|record| record.object_id_hex == commit && record.revision == id.revision)
            .ok_or_else(|| format!("commit {commit} is absent from the SVM source rev_map"))?,
    };
    Ok(ValidatedCommitIdentity { kind, record, id })
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
    use super::{add_info_path_to_url, record_for_first_parent, validated_commit_identity};
    use crate::commands::resolver::{TrackedRevMap, TrackedSvn};
    use crate::config::SvnRemoteConfig;
    use crate::git::GitCli;
    use crate::mapping::build_single_path;
    use crate::rev_map::{ObjectFormat, RevMap, RevMapRecord};
    use crate::tracking_state::TrackingIdentityKind;

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
    fn svm_commit_uses_source_revision_identity() {
        const TRANSPORT: &str = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        const SOURCE: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let temp = tempfile::tempdir().unwrap();
        let git = GitCli::new(temp.path());
        git.init().unwrap();
        git.run_for_test(["config", "user.name", "Test"]).unwrap();
        git.run_for_test(["config", "user.email", "test@example.com"])
            .unwrap();
        git.run_for_test([
            "commit",
            "--allow-empty",
            "-m",
            &format!("import\n\ngit-svn-id: file:///source/trunk@105 {SOURCE}"),
        ])
        .unwrap();
        let oid = git.rev_parse("HEAD").unwrap().trim().to_string();
        let metadata_dir = temp.path().join(".git/svn/refs/remotes/git-svn");
        let transport_path = metadata_dir.join(format!(".rev_map.{TRANSPORT}"));
        let source_path = metadata_dir.join(format!(".rev_map.{SOURCE}"));
        RevMap::open(&transport_path, ObjectFormat::Sha1)
            .unwrap()
            .append(11, &oid)
            .unwrap();
        RevMap::open(&source_path, ObjectFormat::Sha1)
            .unwrap()
            .append(105, &oid)
            .unwrap();
        let tracked = TrackedSvn {
            git,
            config: SvnRemoteConfig::new("svn", "file:///repo", build_single_path("trunk"))
                .with_svm_identity("file:///source", "file:///repo", SOURCE),
            refname: "refs/remotes/git-svn".to_string(),
            svn_path: "trunk".to_string(),
            uuid: TRANSPORT.to_string(),
            rev_map_path: transport_path,
            source_rev_map: Some(TrackedRevMap {
                uuid: SOURCE.to_string(),
                path: source_path,
            }),
        };

        let identity = validated_commit_identity(&tracked, &tracked.records().unwrap(), &oid)
            .expect("valid SVM source identity");

        assert_eq!(identity.kind, TrackingIdentityKind::Source);
        assert_eq!(identity.record.revision, 105);
        assert_eq!(identity.id.url, "file:///source/trunk");
    }

    #[test]
    fn info_url_percent_encodes_each_git_path_segment() {
        assert_eq!(
            add_info_path_to_url("mock://repo/trunk", "dir/space #%.txt").unwrap(),
            "mock://repo/trunk/dir/space%20%23%25.txt"
        );
    }
}
