use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static IMPORT_STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn next_import_staging_ref() -> String {
    let sequence = IMPORT_STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("refs/git-svn-rs/import/{}-{sequence}", std::process::id())
}

pub(super) fn validate_mapping_ref_collisions(mappings: &[&RefMapping]) -> Result<(), String> {
    let mut owners = BTreeMap::<&str, &str>::new();
    for mapping in mappings {
        if let Some(previous_path) =
            owners.insert(mapping.git_ref.as_str(), mapping.svn_path.as_str())
            && previous_path != mapping.svn_path
        {
            return Err(format!(
                "remote ref {} maps both SVN paths {} and {}; configure distinct destinations before fetching",
                mapping.git_ref, previous_path, mapping.svn_path
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_ref_storage_collisions(
    git: &GitCli,
    refnames: &[String],
) -> Result<(), String> {
    let mut all_refnames = git.refs_under("refs")?;
    all_refnames.extend(refnames.iter().cloned());
    validate_refname_namespace(&all_refnames)
}

pub(super) fn validate_refname_namespace(refnames: &[String]) -> Result<(), String> {
    let mut sorted = refnames.to_vec();
    sorted.sort();
    sorted.dedup();

    for (index, left) in sorted.iter().enumerate() {
        for right in &sorted[index + 1..] {
            if right.starts_with(&format!("{left}/")) {
                return Err(format!(
                    "remote refs {left} and {right} cannot coexist because one ref path contains the other"
                ));
            }
        }
    }

    Ok(())
}

pub(super) fn scan_marker_refnames(
    config: &SvnRemoteConfig,
    selected_ref: Option<&str>,
) -> Result<BTreeSet<String>, String> {
    match selected_ref {
        Some(refname) => Ok(BTreeSet::from([refname.to_string()])),
        None => {
            let mut by_svn_path = BTreeMap::new();
            for mapping in &config.fetch {
                by_svn_path.insert(
                    mapping.svn_path.as_str(),
                    sanitize_refname(&mapping.git_ref)?,
                );
            }
            Ok(by_svn_path.into_values().collect())
        }
    }
}

pub(super) fn publish_scan_marker(
    git: &GitCli,
    refname: &str,
    uuid: &str,
    scanned_end: u32,
) -> Result<(), String> {
    if max_imported_revision(git, refname, uuid)? >= scanned_end {
        return Ok(());
    }
    let object_format = git.object_format()?;
    let zero = "0".repeat(object_format.hex_len());
    let current_oid = git
        .rev_parse(refname)
        .ok()
        .map(|oid| oid.trim().to_string())
        .unwrap_or_else(|| zero.clone());
    complete_import_publication(
        git,
        ImportPublication {
            refname: refname.to_string(),
            expected_old_oid: current_oid.clone(),
            target_oid: current_oid,
            rev_maps: vec![ImportRevMapUpdate {
                path: rev_map_path(git, refname, uuid)?,
                records: vec![RevMapRecord {
                    revision: scanned_end,
                    object_id_hex: zero,
                }],
            }],
            append: None,
        },
    )
}

pub(super) fn publish_imported_revisions(
    git: &GitCli,
    refname: &str,
    uuid: &str,
    expected_old_oid: Option<&str>,
    staging_ref: &str,
    revisions: ImportedRevisionMaps<'_>,
    append: Option<ImportAppend>,
) -> Result<(), String> {
    let history = git.first_parent_history(staging_ref)?;
    if history.len() < revisions.transport.len() {
        return Err("import staging ref does not contain every imported revision".to_string());
    }
    if let Some(expected_old_oid) = expected_old_oid
        && history.get(revisions.transport.len()).map(String::as_str) != Some(expected_old_oid)
    {
        return Err(
            "import staging history does not descend from the expected ref tip".to_string(),
        );
    }
    let mut object_ids = history
        .into_iter()
        .take(revisions.transport.len())
        .collect::<Vec<_>>();
    object_ids.reverse();
    let records = revisions
        .transport
        .iter()
        .copied()
        .zip(object_ids.iter().cloned())
        .map(|(revision, object_id_hex)| RevMapRecord {
            revision,
            object_id_hex,
        })
        .collect::<Vec<_>>();
    let target_oid = records
        .last()
        .ok_or_else(|| "import publication has no records".to_string())?
        .object_id_hex
        .clone();
    git.delete_ref_expected(staging_ref, &target_oid)?;
    let object_format = git.object_format()?;
    let expected_old_oid = expected_old_oid
        .map(str::to_string)
        .unwrap_or_else(|| "0".repeat(object_format.hex_len()));
    let mut rev_maps = vec![ImportRevMapUpdate {
        path: rev_map_path(git, refname, uuid)?,
        records,
    }];
    if let Some((source_uuid, source_revisions)) = revisions.source {
        if source_revisions.len() != object_ids.len() {
            return Err("SVM source revision count does not match imported commits".to_string());
        }
        let source_records = source_revisions
            .iter()
            .zip(&object_ids)
            .filter_map(|(revision, object_id_hex)| {
                revision.map(|revision| RevMapRecord {
                    revision,
                    object_id_hex: object_id_hex.clone(),
                })
            })
            .collect::<Vec<_>>();
        if !source_records.is_empty() {
            rev_maps.push(ImportRevMapUpdate {
                path: rev_map_path(git, refname, source_uuid)?,
                records: source_records,
            });
        }
    }
    complete_import_publication(
        git,
        ImportPublication {
            refname: refname.to_string(),
            expected_old_oid,
            target_oid,
            rev_maps,
            append,
        },
    )
}

pub(super) fn rev_map_path(git: &GitCli, refname: &str, uuid: &str) -> Result<PathBuf, String> {
    let git_dir = git.git_dir()?;
    let git_dir = git.work_tree().join(git_dir);
    Ok(svn_metadata_dir(&git_dir, refname)?.join(format!(".rev_map.{uuid}")))
}

pub(super) fn placeholder_ownership(
    git: &GitCli,
    mapping: &RefMapping,
    max_revision: u32,
) -> Result<BTreeSet<String>, String> {
    let metadata_dir = rev_map_path(git, &mapping.git_ref, "metadata")?
        .parent()
        .ok_or_else(|| "rev_map path has no parent directory".to_string())?
        .to_path_buf();
    let mut ownership = BTreeSet::new();
    let compressed = metadata_dir.join("unhandled.log.gz");
    if compressed.exists() {
        let file = std::fs::File::open(&compressed)
            .map_err(|error| format!("failed to read {}: {error}", compressed.display()))?;
        let mut contents = String::new();
        GzDecoder::new(file)
            .read_to_string(&mut contents)
            .map_err(|error| format!("failed to decompress {}: {error}", compressed.display()))?;
        apply_placeholder_log(&contents, &mapping.svn_path, max_revision, &mut ownership)?;
    }
    let current = metadata_dir.join("unhandled.log");
    if current.exists() {
        let contents = std::fs::read_to_string(&current)
            .map_err(|error| format!("failed to read {}: {error}", current.display()))?;
        apply_placeholder_log(&contents, &mapping.svn_path, max_revision, &mut ownership)?;
    }
    Ok(ownership)
}

pub(super) fn apply_placeholder_log(
    contents: &str,
    svn_path: &str,
    max_revision: u32,
    ownership: &mut BTreeSet<String>,
) -> Result<(), String> {
    let mut revision = None;
    for line in contents.lines() {
        if let Some(value) = line.strip_prefix('r') {
            revision = value.parse::<u32>().ok();
            continue;
        }
        if revision.is_none_or(|value| value > max_revision) {
            continue;
        }
        let (present, encoded) = if let Some(path) = line.strip_prefix("  +empty_dir: ") {
            (true, path)
        } else if let Some(path) = line.strip_prefix("  -empty_dir: ") {
            (false, path)
        } else {
            continue;
        };
        let full_path = uri_decode(encoded)?;
        let full_path = full_path.trim_matches('/');
        let svn_path = svn_path.trim_matches('/');
        let relative = if svn_path.is_empty() {
            full_path
        } else if full_path == svn_path {
            ""
        } else if let Some(relative) = full_path.strip_prefix(&format!("{svn_path}/")) {
            relative
        } else {
            continue;
        };
        if relative.is_empty() {
            continue;
        }
        if present {
            ownership.insert(relative.to_string());
        } else {
            ownership.remove(relative);
        }
    }
    Ok(())
}

pub(super) fn uri_decode(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        if index + 2 >= bytes.len() {
            return Err(format!("invalid URI encoding in unhandled.log: {value}"));
        }
        let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
            .map_err(|_| format!("invalid URI encoding in unhandled.log: {value}"))?;
        decoded.push(
            u8::from_str_radix(hex, 16)
                .map_err(|_| format!("invalid URI encoding in unhandled.log: {value}"))?,
        );
        index += 3;
    }
    String::from_utf8(decoded)
        .map_err(|_| format!("non-UTF-8 URI encoding in unhandled.log: {value}"))
}

pub(super) fn unhandled_append(
    git: &GitCli,
    refname: &str,
    revisions: &[(u32, UnhandledMetadata)],
) -> Result<Option<ImportAppend>, String> {
    if revisions.is_empty() {
        return Ok(None);
    }

    let metadata_dir = rev_map_path(git, refname, "metadata")?
        .parent()
        .ok_or_else(|| "rev_map path has no parent directory".to_string())?
        .to_path_buf();
    let mut payload = String::new();
    for (revision, metadata) in revisions {
        payload.push_str(&format!("r{revision}\n"));
        for line in metadata.lines() {
            payload.push_str(&line);
            payload.push('\n');
        }
    }
    Ok(Some(ImportAppend {
        path: metadata_dir.join("unhandled.log"),
        payload: payload.into_bytes(),
    }))
}
