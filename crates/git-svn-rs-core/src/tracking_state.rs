use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::SvnRemoteConfig;
use crate::git::GitCli;
use crate::git_svn_id::GitSvnId;
use crate::mapping::{RefMapping, desanitize_refname, sanitize_refname};
use crate::metadata::svn_metadata_dir;
use crate::rev_map::{RevMap, RevMapRecord};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SelectedRevMaps {
    pub transport_uuid: String,
    pub transport_path: PathBuf,
    pub source: Option<(String, PathBuf)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrackingIdentityKind {
    Transport,
    Source,
}

pub(crate) struct IdentityRevMaps<'a> {
    pub transport_uuid: &'a str,
    pub transport_record: &'a RevMapRecord,
    pub source_path: Option<&'a Path>,
}

pub(crate) fn validate_existing_tracking_state(
    git: &GitCli,
    config: &SvnRemoteConfig,
    refname: &str,
    svn_path: &str,
    uuid: &str,
    rev_map_path: &Path,
) -> Result<Option<RevMapRecord>, String> {
    let metadata_dir = rev_map_path.parent().ok_or_else(|| {
        format!(
            "rev_map path has no metadata directory: {}",
            rev_map_path.display()
        )
    })?;
    let selected = select_existing_rev_maps(git, config, metadata_dir)?
        .ok_or_else(|| format!("missing .rev_map in {}", metadata_dir.display()))?;
    if selected.transport_uuid != uuid || selected.transport_path != rev_map_path {
        return Err(corrupt_error(
            refname,
            rev_map_path,
            "configured transport UUID does not select the requested rev_map",
        ));
    }
    validate_selected_rev_maps(git, config, svn_path, &selected)?;
    let state = read_existing_tracking_state(git, config, refname, rev_map_path)?;
    if let Some(record) = &state.record
        && !tracking_identity_matches(
            git,
            config,
            svn_path,
            IdentityRevMaps {
                transport_uuid: uuid,
                transport_record: record,
                source_path: selected.source.as_ref().map(|(_, path)| path.as_path()),
            },
            state.identity.as_ref(),
        )?
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
        let Some(selected) = select_existing_rev_maps(git, config, &metadata_dir)? else {
            validated.extend(candidates);
            continue;
        };
        let uuid = selected.transport_uuid;
        let rev_map_path = selected.transport_path;
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
        let mut matches = Vec::new();
        for mapping in &candidates {
            if tracking_identity_matches(
                git,
                config,
                &mapping.svn_path,
                IdentityRevMaps {
                    transport_uuid: &uuid,
                    transport_record: record,
                    source_path: selected.source.as_ref().map(|(_, path)| path.as_path()),
                },
                state.identity.as_ref(),
            )? {
                matches.push(mapping.clone());
            }
        }
        match matches.len() {
            1 => validated.push(matches.pop().expect("one matching mapping")),
            0 => {
                if is_importer_auxiliary_ref(
                    git,
                    config,
                    &metadata_root,
                    &refname,
                    &uuid,
                    record,
                    state.identity.as_ref(),
                )? {
                    continue;
                }
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

fn is_importer_auxiliary_ref(
    git: &GitCli,
    config: &SvnRemoteConfig,
    metadata_root: &Path,
    refname: &str,
    uuid: &str,
    record: &RevMapRecord,
    identity: Option<&GitSvnId>,
) -> Result<bool, String> {
    let Some(identity) = identity else {
        return Ok(false);
    };
    if identity.revision != record.revision
        || identity.uuid != config.rewrite_uuid.as_deref().unwrap_or(uuid)
    {
        return Ok(false);
    }
    let Some(base_ref) = auxiliary_base_ref(refname, record.revision) else {
        return Ok(false);
    };
    if !configured_ref_matches(config, base_ref)? {
        return Ok(false);
    }

    let base_metadata_dir = svn_metadata_dir(metadata_root, base_ref)?;
    let Some(base_selected) = select_existing_rev_maps(git, config, &base_metadata_dir)? else {
        return Ok(false);
    };
    let base_uuid = base_selected.transport_uuid;
    let base_rev_map_path = base_selected.transport_path;
    if base_uuid != uuid {
        return Ok(false);
    }
    let base_records = RevMap::open_existing(base_rev_map_path, git.object_format()?)?.records()?;
    for base_record in base_records
        .iter()
        .filter(|base_record| !base_record.has_zero_object_id())
    {
        let history = git.first_parent_history(&base_record.object_id_hex)?;
        if history
            .get(1)
            .is_some_and(|parent| parent == &record.object_id_hex)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn auxiliary_base_ref(refname: &str, revision: u32) -> Option<&str> {
    let marker = format!("@{revision}");
    let marker_start = refname.rfind(&marker)?;
    let suffix = &refname[marker_start + marker.len()..];
    if suffix.bytes().all(|byte| byte == b'-') {
        Some(&refname[..marker_start])
    } else {
        None
    }
}

fn configured_ref_matches(config: &SvnRemoteConfig, refname: &str) -> Result<bool, String> {
    let raw_refname = desanitize_refname(refname);
    for mapping in config
        .fetch
        .iter()
        .chain(config.branches.iter())
        .chain(config.tags.iter())
    {
        let Some((prefix, suffix)) = mapping.git_ref.split_once('*') else {
            if sanitize_refname(&mapping.git_ref)? == refname {
                return Ok(true);
            }
            continue;
        };
        if raw_refname
            .strip_prefix(prefix)
            .and_then(|wildcard| wildcard.strip_suffix(suffix))
            .is_some_and(|wildcard| !wildcard.is_empty())
        {
            return Ok(true);
        }
    }
    Ok(false)
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

pub(crate) fn tracking_identity_matches(
    git: &GitCli,
    config: &SvnRemoteConfig,
    svn_path: &str,
    maps: IdentityRevMaps<'_>,
    identity: Option<&GitSvnId>,
) -> Result<bool, String> {
    Ok(classify_tracking_identity(git, config, svn_path, maps, identity)?.is_some())
}

pub(crate) fn classify_tracking_identity(
    git: &GitCli,
    config: &SvnRemoteConfig,
    svn_path: &str,
    maps: IdentityRevMaps<'_>,
    identity: Option<&GitSvnId>,
) -> Result<Option<TrackingIdentityKind>, String> {
    if config.no_metadata {
        return Ok(Some(TrackingIdentityKind::Transport));
    }
    let Some(identity) = identity else {
        return Ok(None);
    };
    if identity.revision == maps.transport_record.revision
        && identity.url == config.metadata_url(svn_path)?
        && identity.uuid == config.metadata_uuid(maps.transport_uuid)?
    {
        return Ok(Some(TrackingIdentityKind::Transport));
    }
    let Some(source_path) = maps.source_path else {
        return Ok(None);
    };
    let Some(source_uuid) = config.svm_uuid.as_deref() else {
        return Ok(None);
    };
    if identity.uuid != source_uuid || identity.url != svm_metadata_url(config, svn_path)? {
        return Ok(None);
    }
    Ok(RevMap::open_existing(source_path, git.object_format()?)?
        .records()?
        .iter()
        .any(|source_record| {
            source_record.revision == identity.revision
                && source_record.object_id_hex == maps.transport_record.object_id_hex
                && !source_record.has_zero_object_id()
        })
        .then_some(TrackingIdentityKind::Source))
}

pub(crate) fn svm_metadata_url(config: &SvnRemoteConfig, svn_path: &str) -> Result<String, String> {
    let transport_url = config.metadata_url(svn_path)?;
    let replace = config.svm_replace.as_deref().ok_or_else(|| {
        format!(
            "svn-remote.{} uses useSvmProps but has no validated svm-replace",
            config.name
        )
    })?;
    let source = config.svm_source.as_deref().ok_or_else(|| {
        format!(
            "svn-remote.{} uses useSvmProps but has no validated svm-source",
            config.name
        )
    })?;
    let suffix = transport_url.strip_prefix(replace).ok_or_else(|| {
        format!("SVM replacement URL {replace:?} does not prefix {transport_url:?}")
    })?;
    if !suffix.is_empty() && !suffix.starts_with('/') {
        return Err(format!(
            "SVM replacement URL {replace:?} is not a path-boundary prefix of {transport_url:?}"
        ));
    }
    Ok(format!("{}{}", source.trim_end_matches('/'), suffix))
}

pub(crate) fn validate_selected_rev_maps(
    git: &GitCli,
    config: &SvnRemoteConfig,
    svn_path: &str,
    selected: &SelectedRevMaps,
) -> Result<(), String> {
    let Some((source_uuid, source_path)) = &selected.source else {
        return Ok(());
    };
    let transport_records =
        RevMap::open_existing(&selected.transport_path, git.object_format()?)?.records()?;
    let expected_url = svm_metadata_url(config, svn_path)?;
    for record in RevMap::open_existing(source_path, git.object_format()?)?.records()? {
        if record.has_zero_object_id() {
            continue;
        }
        if !transport_records
            .iter()
            .any(|transport| transport.object_id_hex == record.object_id_hex)
        {
            return Err(format!(
                "source rev_map {} r{} points to {}, which is absent from transport rev_map {}",
                source_path.display(),
                record.revision,
                record.object_id_hex,
                selected.transport_path.display()
            ));
        }
        let message = git.commit_message(&record.object_id_hex)?;
        let identity = message
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .and_then(|line| GitSvnId::parse(line.trim_end_matches('\r')).ok())
            .ok_or_else(|| {
                format!(
                    "source rev_map {} r{} points to a commit without a valid git-svn-id footer",
                    source_path.display(),
                    record.revision
                )
            })?;
        if identity.revision != record.revision
            || identity.uuid != *source_uuid
            || identity.url != expected_url
        {
            return Err(format!(
                "source rev_map {} r{} disagrees with {}",
                source_path.display(),
                record.revision,
                identity.to_footer()
            ));
        }
    }
    Ok(())
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
    let expected_url = config
        .metadata_url(svn_path)
        .unwrap_or_else(|error| format!("<invalid metadata URL: {error}>"));
    let expected_uuid = config
        .metadata_uuid(uuid)
        .unwrap_or("<invalid metadata UUID>");
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

pub(crate) fn select_existing_rev_maps(
    git: &GitCli,
    config: &SvnRemoteConfig,
    metadata_dir: &Path,
) -> Result<Option<SelectedRevMaps>, String> {
    let candidates = rev_map_candidates(metadata_dir)?;
    if candidates.is_empty() {
        return Ok(None);
    }
    if !config.use_svm_props {
        return match candidates.as_slice() {
            [(uuid, path)] => Ok(Some(SelectedRevMaps {
                transport_uuid: uuid.clone(),
                transport_path: path.clone(),
                source: None,
            })),
            _ => Err(ambiguous_rev_maps(metadata_dir, &candidates)),
        };
    }
    let prefix = format!("svn-remote.{}", config.name);
    let transport_uuid = git
        .git_svn_metadata_get(&format!("{prefix}.uuid"))?
        .ok_or_else(|| format!("missing private {prefix}.uuid for useSvmProps tracking"))?;
    let source_uuid = config.svm_uuid.clone().ok_or_else(|| {
        format!(
            "svn-remote.{} uses useSvmProps but has no validated svm-uuid",
            config.name
        )
    })?;
    if transport_uuid == source_uuid {
        return Err("SVM transport UUID and source UUID select the same rev_map".to_string());
    }
    let unexpected = candidates
        .iter()
        .filter(|(uuid, _)| uuid != &transport_uuid && uuid != &source_uuid)
        .collect::<Vec<_>>();
    if !unexpected.is_empty() {
        return Err(ambiguous_rev_maps(metadata_dir, &candidates));
    }
    let path_for = |uuid: &str| {
        candidates
            .iter()
            .find(|(candidate, _)| candidate == uuid)
            .map(|(_, path)| path.clone())
            .ok_or_else(|| {
                format!(
                    "missing configured .rev_map.{uuid} in {}",
                    metadata_dir.display()
                )
            })
    };
    let transport_path = path_for(&transport_uuid)?;
    let source = candidates
        .iter()
        .any(|(candidate, _)| candidate == &source_uuid)
        .then(|| path_for(&source_uuid).map(|path| (source_uuid.clone(), path)))
        .transpose()?;
    Ok(Some(SelectedRevMaps {
        transport_uuid: transport_uuid.clone(),
        transport_path,
        source,
    }))
}

fn rev_map_candidates(metadata_dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let entries = match std::fs::read_dir(metadata_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
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
    Ok(candidates)
}

fn ambiguous_rev_maps(metadata_dir: &Path, candidates: &[(String, PathBuf)]) -> String {
    format!(
        "ambiguous .rev_map files in {}: {}",
        metadata_dir.display(),
        candidates
            .iter()
            .map(|(_, path)| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    )
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
    use super::{
        select_existing_rev_maps, validate_candidate_mappings, validate_existing_tracking_state,
    };
    use crate::config::SvnRemoteConfig;
    use crate::git::GitCli;
    use crate::mapping::{LayoutMappings, MappingKind, RefMapping, build_single_path};
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

    #[test]
    fn accepts_configured_svm_transport_and_source_rev_maps() {
        const TRANSPORT: &str = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        const SOURCE: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let mut fixture = fixture(&format!(
            "import\n\ngit-svn-id: file:///source/trunk@105 {SOURCE}"
        ));
        fixture.config = fixture
            .config
            .with_svm_identity("file:///source", "file:///repo", SOURCE);
        fixture
            .git
            .git_svn_metadata_set("svn-remote.svn.uuid", TRANSPORT)
            .unwrap();
        fixture.rev_map_path = fixture
            .rev_map_path
            .with_file_name(format!(".rev_map.{TRANSPORT}"));
        RevMap::open(&fixture.rev_map_path, ObjectFormat::Sha1)
            .unwrap()
            .append(11, &fixture.oid)
            .unwrap();
        let source_path = fixture
            .rev_map_path
            .with_file_name(format!(".rev_map.{SOURCE}"));
        RevMap::open(&source_path, ObjectFormat::Sha1)
            .unwrap()
            .append(105, &fixture.oid)
            .unwrap();

        validate(&fixture, TRANSPORT).unwrap();
        let selected = select_existing_rev_maps(
            &fixture.git,
            &fixture.config,
            fixture.rev_map_path.parent().unwrap(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(selected.transport_path, fixture.rev_map_path);
        assert_eq!(selected.source, Some((SOURCE.to_string(), source_path)));
    }

    #[test]
    fn accepts_transport_only_svm_map_before_first_source_revision() {
        const TRANSPORT: &str = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        const SOURCE: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let mut fixture = fixture(&format!(
            "import\n\ngit-svn-id: file:///repo/trunk@11 {TRANSPORT}"
        ));
        fixture.config =
            fixture
                .config
                .with_svm_identity("file:///source/trunk", "file:///repo/trunk", SOURCE);
        fixture
            .git
            .git_svn_metadata_set("svn-remote.svn.uuid", TRANSPORT)
            .unwrap();
        fixture.rev_map_path = fixture
            .rev_map_path
            .with_file_name(format!(".rev_map.{TRANSPORT}"));
        RevMap::open(&fixture.rev_map_path, ObjectFormat::Sha1)
            .unwrap()
            .append(11, &fixture.oid)
            .unwrap();

        validate(&fixture, TRANSPORT).unwrap();
        let selected = select_existing_rev_maps(
            &fixture.git,
            &fixture.config,
            fixture.rev_map_path.parent().unwrap(),
        )
        .unwrap()
        .unwrap();
        assert!(selected.source.is_none());
    }

    #[test]
    fn rejects_source_footer_without_map_and_extra_or_tampered_source_maps() {
        const TRANSPORT: &str = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        const SOURCE: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let mut fixture = fixture(&format!(
            "import\n\ngit-svn-id: file:///source/trunk@105 {SOURCE}"
        ));
        fixture.config = fixture
            .config
            .with_svm_identity("file:///source", "file:///repo", SOURCE);
        fixture
            .git
            .git_svn_metadata_set("svn-remote.svn.uuid", TRANSPORT)
            .unwrap();
        fixture.rev_map_path = fixture
            .rev_map_path
            .with_file_name(format!(".rev_map.{TRANSPORT}"));
        RevMap::open(&fixture.rev_map_path, ObjectFormat::Sha1)
            .unwrap()
            .append(11, &fixture.oid)
            .unwrap();

        let missing = validate(&fixture, TRANSPORT).unwrap_err();
        assert!(missing.contains("rev_map r11 expects"), "{missing}");

        let source_path = fixture
            .rev_map_path
            .with_file_name(format!(".rev_map.{SOURCE}"));
        RevMap::open(&source_path, ObjectFormat::Sha1)
            .unwrap()
            .append(105, &"c".repeat(40))
            .unwrap();
        let tampered = validate(&fixture, TRANSPORT).unwrap_err();
        assert!(
            tampered.contains("absent from transport rev_map"),
            "{tampered}"
        );

        RevMap::open(
            fixture.rev_map_path.with_file_name(".rev_map.unexpected"),
            ObjectFormat::Sha1,
        )
        .unwrap()
        .append(1, &fixture.oid)
        .unwrap();
        let extra = validate(&fixture, TRANSPORT).unwrap_err();
        assert!(extra.contains("ambiguous .rev_map files"), "{extra}");
    }

    #[test]
    fn candidate_validation_skips_importer_auxiliary_refs() {
        let (_temp, git, config, candidates) =
            auxiliary_candidate_fixture("git-svn-id: file:///repo/legacy@1 repo-uuid");

        assert!(
            validate_candidate_mappings(&git, &config, candidates)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn auxiliary_detection_does_not_hide_revision_or_uuid_corruption() {
        for footer in [
            "git-svn-id: file:///repo/legacy@2 repo-uuid",
            "git-svn-id: file:///repo/legacy@1 other-uuid",
        ] {
            let (_temp, git, config, candidates) = auxiliary_candidate_fixture(footer);
            let error = validate_candidate_mappings(&git, &config, candidates).unwrap_err();
            assert!(
                error.contains("git-svn-id does not match any configured SVN path"),
                "{error}"
            );
        }
    }

    fn auxiliary_candidate_fixture(
        footer: &str,
    ) -> (tempfile::TempDir, GitCli, SvnRemoteConfig, Vec<RefMapping>) {
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
            &format!("source\n\n{footer}"),
        ])
        .unwrap();
        let auxiliary_oid = git.rev_parse("HEAD").unwrap().trim().to_string();
        let auxiliary_ref = "refs/remotes/origin/topic@1";
        git.update_ref(auxiliary_ref, &auxiliary_oid).unwrap();
        git.run_for_test([
            "commit",
            "--allow-empty",
            "-m",
            "branch\n\ngit-svn-id: file:///repo/branches/topic@2 repo-uuid",
        ])
        .unwrap();
        let branch_oid = git.rev_parse("HEAD").unwrap().trim().to_string();
        let branch_ref = "refs/remotes/origin/topic";
        git.update_ref(branch_ref, &branch_oid).unwrap();

        for (refname, revision, oid) in [
            (auxiliary_ref, 1, auxiliary_oid.as_str()),
            (branch_ref, 2, branch_oid.as_str()),
        ] {
            let path = temp
                .path()
                .join(".git/svn")
                .join(refname)
                .join(".rev_map.repo-uuid");
            RevMap::open(path, ObjectFormat::Sha1)
                .unwrap()
                .append(revision, oid)
                .unwrap();
        }
        let candidates = vec![RefMapping {
            kind: MappingKind::Branches,
            svn_path: "branches/topic@1".to_string(),
            git_ref: auxiliary_ref.to_string(),
        }];
        (temp, git, wildcard_config(), candidates)
    }

    #[test]
    fn candidate_validation_keeps_real_branch_names_ending_in_at_revision() {
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
            "real branch\n\ngit-svn-id: file:///repo/branches/topic@1@1 repo-uuid",
        ])
        .unwrap();
        let oid = git.rev_parse("HEAD").unwrap().trim().to_string();
        let refname = "refs/remotes/origin/topic@1";
        git.update_ref(refname, &oid).unwrap();
        let path = temp
            .path()
            .join(".git/svn")
            .join(refname)
            .join(".rev_map.repo-uuid");
        RevMap::open(path, ObjectFormat::Sha1)
            .unwrap()
            .append(1, &oid)
            .unwrap();
        let mapping = RefMapping {
            kind: MappingKind::Branches,
            svn_path: "branches/topic@1".to_string(),
            git_ref: refname.to_string(),
        };

        assert_eq!(
            validate_candidate_mappings(&git, &wildcard_config(), vec![mapping.clone()]).unwrap(),
            vec![mapping]
        );
    }

    fn wildcard_config() -> SvnRemoteConfig {
        SvnRemoteConfig::new(
            "svn",
            "file:///repo",
            LayoutMappings {
                fetch: Vec::new(),
                branches: vec![RefMapping {
                    kind: MappingKind::Branches,
                    svn_path: "branches/*".to_string(),
                    git_ref: "refs/remotes/origin/*".to_string(),
                }],
                tags: Vec::new(),
            },
        )
    }
}
