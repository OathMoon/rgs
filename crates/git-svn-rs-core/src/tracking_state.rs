use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::SvnRemoteConfig;
use crate::git::GitCli;
use crate::git_svn_id::GitSvnId;
use crate::mapping::RefMapping;
use crate::metadata::svn_metadata_dir;
use crate::rev_map::{RevMap, RevMapRecord};

pub(crate) fn validate_existing_tracking_state(
    git: &GitCli,
    config: &SvnRemoteConfig,
    refname: &str,
    svn_path: &str,
    uuid: &str,
    rev_map_path: &Path,
) -> Result<Option<RevMapRecord>, String> {
    let state = read_existing_tracking_state(git, config, refname, rev_map_path)?;
    if let Some(record) = &state.record
        && !tracking_identity_matches(config, svn_path, uuid, record, state.identity.as_ref())
    {
        return Err(identity_mismatch_error(
            config,
            refname,
            svn_path,
            uuid,
            rev_map_path,
            record,
            state.identity.as_ref(),
        ));
    }
    Ok(state.record)
}

pub(crate) fn validate_candidate_mappings(
    git: &GitCli,
    config: &SvnRemoteConfig,
    mappings: Vec<RefMapping>,
) -> Result<Vec<RefMapping>, String> {
    let git_dir = git.git_dir()?;
    let metadata_root = git.work_tree().join(git_dir);
    let mut by_ref = BTreeMap::<String, Vec<RefMapping>>::new();
    for mapping in mappings {
        let candidates = by_ref.entry(mapping.git_ref.clone()).or_default();
        if !candidates
            .iter()
            .any(|candidate| candidate.svn_path == mapping.svn_path)
        {
            candidates.push(mapping);
        }
    }

    let mut validated = Vec::new();
    for (refname, candidates) in by_ref {
        let metadata_dir = svn_metadata_dir(&metadata_root, &refname)?;
        let Some((uuid, rev_map_path)) = find_existing_rev_map(&metadata_dir)? else {
            validated.extend(candidates);
            continue;
        };
        let state = read_existing_tracking_state(git, config, &refname, &rev_map_path)?;
        let Some(record) = state.record.as_ref() else {
            if candidates.len() > 1 {
                return Err(format!(
                    "ambiguous SVN mappings for {refname} with no commit identity"
                ));
            }
            validated.extend(candidates);
            continue;
        };
        let mut matches = candidates
            .iter()
            .filter(|mapping| {
                tracking_identity_matches(
                    config,
                    &mapping.svn_path,
                    &uuid,
                    record,
                    state.identity.as_ref(),
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        match matches.len() {
            1 => validated.push(matches.pop().expect("one matching mapping")),
            0 => {
                let paths = candidates
                    .iter()
                    .map(|mapping| mapping.svn_path.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(corrupt_error(
                    &refname,
                    &rev_map_path,
                    format!("git-svn-id does not match any configured SVN path: {paths}"),
                ));
            }
            _ => {
                return Err(format!(
                    "ambiguous SVN tracking identity for {refname}: multiple mappings match {}",
                    state
                        .identity
                        .as_ref()
                        .map(GitSvnId::to_footer)
                        .unwrap_or_else(|| "noMetadata state".to_string())
                ));
            }
        }
    }
    Ok(validated)
}

struct ExistingTrackingState {
    record: Option<RevMapRecord>,
    identity: Option<GitSvnId>,
}

fn read_existing_tracking_state(
    git: &GitCli,
    config: &SvnRemoteConfig,
    refname: &str,
    rev_map_path: &Path,
) -> Result<ExistingTrackingState, String> {
    let records = RevMap::open_existing(rev_map_path, git.object_format()?)?.records()?;
    let record = records
        .iter()
        .rev()
        .find(|record| !record.has_zero_object_id())
        .cloned();
    let ref_oid = current_ref_oid(git, refname)?;

    let (record, ref_oid) = match (record, ref_oid) {
        (None, None) => {
            return Ok(ExistingTrackingState {
                record: None,
                identity: None,
            });
        }
        (None, Some(ref_oid)) => {
            return Err(corrupt_error(
                refname,
                rev_map_path,
                format!("tracking ref points to {ref_oid}, but the rev_map has no commit record"),
            ));
        }
        (Some(_), None) => {
            return Err(corrupt_error(
                refname,
                rev_map_path,
                "rev_map has a commit record, but the tracking ref is missing",
            ));
        }
        (Some(record), Some(ref_oid)) => (record, ref_oid),
    };

    if ref_oid != record.object_id_hex {
        return Err(corrupt_error(
            refname,
            rev_map_path,
            format!(
                "tracking ref points to {ref_oid}, but rev_map r{} points to {}",
                record.revision, record.object_id_hex
            ),
        ));
    }
    if config.no_metadata {
        return Ok(ExistingTrackingState {
            record: Some(record),
            identity: None,
        });
    }

    let message = git.commit_message(&record.object_id_hex)?;
    let footer = message
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| {
            corrupt_error(
                refname,
                rev_map_path,
                format!("commit {} has no git-svn-id footer", record.object_id_hex),
            )
        })?;
    let identity = GitSvnId::parse(footer.trim_end_matches('\r')).map_err(|_| {
        corrupt_error(
            refname,
            rev_map_path,
            format!(
                "commit {} has no valid git-svn-id footer",
                record.object_id_hex
            ),
        )
    })?;
    Ok(ExistingTrackingState {
        record: Some(record),
        identity: Some(identity),
    })
}

fn tracking_identity_matches(
    config: &SvnRemoteConfig,
    svn_path: &str,
    uuid: &str,
    record: &RevMapRecord,
    identity: Option<&GitSvnId>,
) -> bool {
    if config.no_metadata {
        return true;
    }
    let Some(identity) = identity else {
        return false;
    };
    identity.revision == record.revision
        && identity.url == config.metadata_url(svn_path)
        && identity.uuid == config.rewrite_uuid.as_deref().unwrap_or(uuid)
}

fn identity_mismatch_error(
    config: &SvnRemoteConfig,
    refname: &str,
    svn_path: &str,
    uuid: &str,
    rev_map_path: &Path,
    record: &RevMapRecord,
    identity: Option<&GitSvnId>,
) -> String {
    let expected_url = config.metadata_url(svn_path);
    let expected_uuid = config.rewrite_uuid.as_deref().unwrap_or(uuid);
    corrupt_error(
        refname,
        rev_map_path,
        format!(
            "rev_map r{} expects git-svn-id {}@{} {}, found {}",
            record.revision,
            expected_url,
            record.revision,
            expected_uuid,
            identity
                .map(GitSvnId::to_footer)
                .unwrap_or_else(|| "no valid footer".to_string())
        ),
    )
}

pub(crate) fn find_existing_rev_map(
    metadata_dir: &Path,
) -> Result<Option<(String, PathBuf)>, String> {
    let entries = match std::fs::read_dir(metadata_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let mut candidates = Vec::new();
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if let Some(uuid) = name.strip_prefix(".rev_map.")
            && !uuid.ends_with(".lock")
            && path.is_file()
        {
            candidates.push((uuid.to_string(), path));
        }
    }
    candidates.sort_by(|left, right| left.1.cmp(&right.1));
    match candidates.len() {
        0 => Ok(None),
        1 => Ok(candidates.pop()),
        _ => Err(format!(
            "ambiguous .rev_map files in {}: {}",
            metadata_dir.display(),
            candidates
                .iter()
                .map(|(_, path)| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn current_ref_oid(git: &GitCli, refname: &str) -> Result<Option<String>, String> {
    if git.refs_under(refname)?.iter().any(|name| name == refname) {
        Ok(Some(git.rev_parse(refname)?.trim().to_string()))
    } else {
        Ok(None)
    }
}

fn corrupt_error(refname: &str, rev_map_path: &Path, detail: impl std::fmt::Display) -> String {
    format!(
        "corrupt SVN tracking state for {refname} and {}: {detail}",
        rev_map_path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::validate_existing_tracking_state;
    use crate::config::SvnRemoteConfig;
    use crate::git::GitCli;
    use crate::mapping::build_single_path;
    use crate::rev_map::{ObjectFormat, RevMap};

    const REFNAME: &str = "refs/remotes/git-svn";

    struct Fixture {
        _temp: tempfile::TempDir,
        git: GitCli,
        config: SvnRemoteConfig,
        rev_map_path: std::path::PathBuf,
        oid: String,
    }

    fn fixture(message: &str) -> Fixture {
        let temp = tempfile::tempdir().unwrap();
        let git = GitCli::new(temp.path());
        git.init().unwrap();
        git.run_for_test(["config", "user.name", "Test"]).unwrap();
        git.run_for_test(["config", "user.email", "test@example.com"])
            .unwrap();
        git.run_for_test(["commit", "--allow-empty", "-m", message])
            .unwrap();
        let oid = git.rev_parse("HEAD").unwrap().trim().to_string();
        git.update_ref(REFNAME, &oid).unwrap();
        Fixture {
            rev_map_path: temp
                .path()
                .join(".git/svn/refs/remotes/git-svn/.rev_map.repo-uuid"),
            _temp: temp,
            git,
            config: SvnRemoteConfig::new("svn", "file:///repo/trunk", build_single_path("")),
            oid,
        }
    }

    fn validate(fixture: &Fixture, uuid: &str) -> Result<(), String> {
        validate_existing_tracking_state(
            &fixture.git,
            &fixture.config,
            REFNAME,
            "",
            uuid,
            &fixture.rev_map_path,
        )
        .map(|_| ())
    }

    #[test]
    fn accepts_commit_tail_before_trailing_zero_scan_marker() {
        let fixture = fixture("import\n\ngit-svn-id: file:///repo/trunk@1 repo-uuid");
        let mut rev_map = RevMap::open(&fixture.rev_map_path, ObjectFormat::Sha1).unwrap();
        rev_map.append(1, &fixture.oid).unwrap();
        rev_map.append(2, &"0".repeat(40)).unwrap();

        validate(&fixture, "repo-uuid").unwrap();
    }

    #[test]
    fn accepts_zero_only_map_with_absent_ref() {
        let temp = tempfile::tempdir().unwrap();
        let git = GitCli::new(temp.path());
        git.init().unwrap();
        let config = SvnRemoteConfig::new("svn", "file:///repo/trunk", build_single_path(""));
        let path = temp
            .path()
            .join(".git/svn/refs/remotes/git-svn/.rev_map.repo-uuid");
        let mut rev_map = RevMap::open(&path, ObjectFormat::Sha1).unwrap();
        rev_map.append(2, &"0".repeat(40)).unwrap();

        assert!(
            validate_existing_tracking_state(&git, &config, REFNAME, "", "repo-uuid", &path,)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn rejects_revision_url_and_uuid_footer_mismatches() {
        for (footer, expected) in [
            (
                "git-svn-id: file:///repo/trunk@2 repo-uuid",
                "rev_map r1 expects",
            ),
            (
                "git-svn-id: file:///other/trunk@1 repo-uuid",
                "rev_map r1 expects",
            ),
            (
                "git-svn-id: file:///repo/trunk@1 other-uuid",
                "rev_map r1 expects",
            ),
        ] {
            let fixture = fixture(&format!("import\n\n{footer}"));
            let mut rev_map = RevMap::open(&fixture.rev_map_path, ObjectFormat::Sha1).unwrap();
            rev_map.append(1, &fixture.oid).unwrap();

            let error = validate(&fixture, "repo-uuid").unwrap_err();
            assert!(error.contains(expected), "{error}");
        }
    }

    #[test]
    fn rejects_tracking_ref_tip_mismatch() {
        let fixture = fixture("import\n\ngit-svn-id: file:///repo/trunk@1 repo-uuid");
        let mut rev_map = RevMap::open(&fixture.rev_map_path, ObjectFormat::Sha1).unwrap();
        rev_map.append(1, &fixture.oid).unwrap();
        fixture
            .git
            .run_for_test(["commit", "--allow-empty", "-m", "local"])
            .unwrap();
        let local_oid = fixture.git.rev_parse("HEAD").unwrap();
        fixture.git.update_ref(REFNAME, local_oid.trim()).unwrap();

        let error = validate(&fixture, "repo-uuid").unwrap_err();
        assert!(error.contains("tracking ref points to"), "{error}");
    }

    #[test]
    fn no_metadata_skips_only_footer_validation() {
        let mut fixture = fixture("import without metadata");
        fixture.config.no_metadata = true;
        let mut rev_map = RevMap::open(&fixture.rev_map_path, ObjectFormat::Sha1).unwrap();
        rev_map.append(1, &fixture.oid).unwrap();

        validate(&fixture, "repo-uuid").unwrap();
    }
}
