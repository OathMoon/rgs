use crate::git::GitCli;
use crate::rev_map::{ObjectFormat, RevMap, RevMapRecord};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const MAGIC_V1: &str = "git-svn-rs-import-journal-v1";
const MAGIC: &str = "git-svn-rs-import-journal-v2";
const JOURNAL_FILE: &str = "import-journal";
const LOCK_FILE: &str = "import.lock";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportAppend {
    pub path: PathBuf,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPublication {
    pub refname: String,
    pub expected_old_oid: String,
    pub target_oid: String,
    pub rev_map_path: PathBuf,
    pub records: Vec<RevMapRecord>,
    pub append: Option<ImportAppend>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportJournal {
    publication: ImportPublication,
    original_record_count: usize,
    original_tail: Option<RevMapRecord>,
    append_original_len: usize,
    append_original_sha256: String,
    append_payload_len: usize,
    append_payload_sha256: String,
}

/// Persist and complete one mapping's ref/rev_map publication.
///
/// Git objects must already exist, but the final ref must still equal
/// `expected_old_oid`. Once the journal is durable, recovery can finish every
/// state produced by this function without recreating commits.
pub fn complete(git: &GitCli, publication: ImportPublication) -> Result<(), String> {
    with_exclusive_lock(git, || complete_locked(git, publication))
}

fn complete_locked(git: &GitCli, publication: ImportPublication) -> Result<(), String> {
    let path = journal_path(git)?;
    if path.exists() {
        return Err(format!(
            "unfinished import journal found at {}; recover it before publishing another import",
            path.display()
        ));
    }
    validate_publication(git, &publication)?;
    let format = git.object_format()?;
    let existing = existing_records(&publication.rev_map_path, format)?;
    validate_new_records(&existing, &publication.records)?;
    let append_original = publication
        .append
        .as_ref()
        .map(|append| read_or_empty(&append.path))
        .transpose()?
        .unwrap_or_default();
    let journal = ImportJournal {
        append_payload_len: publication
            .append
            .as_ref()
            .map_or(0, |append| append.payload.len()),
        append_payload_sha256: publication
            .append
            .as_ref()
            .map_or_else(|| sha256(&[]), |append| sha256(&append.payload)),
        publication,
        original_record_count: existing.len(),
        original_tail: existing.last().cloned(),
        append_original_len: append_original.len(),
        append_original_sha256: sha256(&append_original),
    };
    save(&path, &journal)?;
    finish(git, &path, &journal)
}

/// Resume a durable import publication. Returns `true` when a journal existed.
pub fn recover_pending(git: &GitCli) -> Result<bool, String> {
    with_exclusive_lock(git, || recover_pending_locked(git))
}

fn recover_pending_locked(git: &GitCli) -> Result<bool, String> {
    let path = journal_path(git)?;
    let Some(journal) = load(&path)? else {
        return Ok(false);
    };
    validate_publication(git, &journal.publication)?;
    finish(git, &path, &journal)?;
    Ok(true)
}

pub fn ensure_no_pending(git: &GitCli) -> Result<(), String> {
    let path = journal_path(git)?;
    if path.exists() {
        Err(format!(
            "unfinished import journal found at {}; run fetch again to recover it",
            path.display()
        ))
    } else {
        Ok(())
    }
}

fn finish(git: &GitCli, path: &Path, journal: &ImportJournal) -> Result<(), String> {
    let publication = &journal.publication;
    validate_append_prefix(journal)?;
    let current = current_ref_oid(git, &publication.refname)?;
    if current == publication.expected_old_oid {
        git.update_ref_expected(
            &publication.refname,
            &publication.target_oid,
            &publication.expected_old_oid,
        )?;
    } else if current != publication.target_oid {
        return Err(format!(
            "cannot recover import publication: {} moved to {current}, expected {} or {}",
            publication.refname, publication.expected_old_oid, publication.target_oid
        ));
    }

    let format = git.object_format()?;
    let existing = existing_records(&publication.rev_map_path, format)?;
    validate_recovery_prefix(journal, &existing)?;
    let mut rev_map = if publication.rev_map_path.exists() {
        RevMap::open_existing(&publication.rev_map_path, format)?
    } else {
        RevMap::open(&publication.rev_map_path, format)?
    };
    let appended = existing.len() - journal.original_record_count;
    for record in publication.records.iter().skip(appended) {
        rev_map.append(record.revision, &record.object_id_hex)?;
    }

    let final_records = rev_map.records()?;
    validate_recovery_prefix(journal, &final_records)?;
    if final_records.len() != journal.original_record_count + publication.records.len() {
        return Err("import rev_map verification did not reach the journal target".to_string());
    }
    let final_ref = current_ref_oid(git, &publication.refname)?;
    if final_ref != publication.target_oid {
        return Err(format!(
            "import ref verification found {final_ref}, expected {}",
            publication.target_oid
        ));
    }
    finish_append(journal)?;
    remove(path)
}

fn validate_publication(git: &GitCli, publication: &ImportPublication) -> Result<(), String> {
    if publication.refname.is_empty() || publication.records.is_empty() {
        return Err(
            "import publication requires a ref and at least one rev_map record".to_string(),
        );
    }
    let format = git.object_format()?;
    validate_oid(&publication.expected_old_oid, format)?;
    validate_oid(&publication.target_oid, format)?;
    for record in &publication.records {
        validate_oid(&record.object_id_hex, format)?;
    }
    if publication
        .records
        .last()
        .map(|record| &record.object_id_hex)
        != Some(&publication.target_oid)
    {
        return Err(
            "import publication target does not match the final rev_map record".to_string(),
        );
    }
    let root = svn_metadata_root(git)?;
    if !publication.rev_map_path.starts_with(&root) {
        return Err(format!(
            "import journal rev_map path escapes SVN metadata root: {}",
            publication.rev_map_path.display()
        ));
    }
    ensure_no_symlink_components(&root, &publication.rev_map_path)?;
    if let Some(append) = &publication.append {
        let expected = publication
            .rev_map_path
            .parent()
            .ok_or_else(|| "import rev_map path has no parent".to_string())?
            .join("unhandled.log");
        if append.path != expected {
            return Err(format!(
                "import journal append path does not match its rev_map: {}",
                append.path.display()
            ));
        }
        ensure_no_symlink_components(&root, &append.path)?;
    }
    Ok(())
}

fn ensure_no_symlink_components(root: &Path, target: &Path) -> Result<(), String> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| "import metadata path escapes SVN metadata root".to_string())?;
    let mut current = root.to_path_buf();
    check_not_symlink(&current)?;
    for component in relative.components() {
        if let std::path::Component::Normal(component) = component {
            current.push(component);
        } else {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "import metadata path contains a symbolic link: {}",
                    current.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

fn check_not_symlink(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "import metadata path contains a symbolic link: {}",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn finish_append(journal: &ImportJournal) -> Result<(), String> {
    let Some(append) = &journal.publication.append else {
        if journal.append_original_len != 0 || journal.append_original_sha256 != sha256(&[]) {
            return Err("import journal has unexpected append state".to_string());
        }
        return Ok(());
    };
    let existing = validate_append_prefix(journal)?;
    let appended = &existing[journal.append_original_len..];
    if appended.len() < append.payload.len() {
        let parent = append
            .path
            .parent()
            .ok_or_else(|| "import append target has no parent".to_string())?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&append.path)
            .map_err(|error| error.to_string())?;
        file.write_all(&append.payload[appended.len()..])
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        sync_directory(parent)?;
    }
    let final_bytes = read_or_empty(&append.path)?;
    if final_bytes.len() != journal.append_original_len + append.payload.len()
        || sha256(&final_bytes[..journal.append_original_len]) != journal.append_original_sha256
        || final_bytes[journal.append_original_len..] != append.payload
    {
        return Err(
            "import append target verification did not reach the journal target".to_string(),
        );
    }
    Ok(())
}

fn validate_append_prefix(journal: &ImportJournal) -> Result<Vec<u8>, String> {
    let Some(append) = &journal.publication.append else {
        if journal.append_original_len != 0
            || journal.append_original_sha256 != sha256(&[])
            || journal.append_payload_len != 0
            || journal.append_payload_sha256 != sha256(&[])
        {
            return Err("import journal has unexpected append state".to_string());
        }
        return Ok(Vec::new());
    };
    if append.payload.len() != journal.append_payload_len
        || sha256(&append.payload) != journal.append_payload_sha256
    {
        return Err("import journal append payload checksum mismatch".to_string());
    }
    let existing = read_or_empty(&append.path)?;
    if existing.len() < journal.append_original_len {
        return Err("import append target was truncated after the journal was written".to_string());
    }
    if sha256(&existing[..journal.append_original_len]) != journal.append_original_sha256 {
        return Err("import append target prefix does not match the journal".to_string());
    }
    let appended = &existing[journal.append_original_len..];
    if appended.len() > append.payload.len() || !append.payload.starts_with(appended) {
        return Err("import append target suffix does not match the journal".to_string());
    }
    Ok(existing)
}

fn read_or_empty(path: &Path) -> Result<Vec<u8>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.to_string()),
    }
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn with_exclusive_lock<T>(
    git: &GitCli,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let root = svn_metadata_root(git)?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let path = root.join(LOCK_FILE);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| error.to_string())?;
    file.try_lock_exclusive()
        .map_err(|error| format!("cannot lock import recovery at {}: {error}", path.display()))?;
    operation()
}

fn validate_oid(oid: &str, format: ObjectFormat) -> Result<(), String> {
    if oid.len() != format.hex_len() || !oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "invalid {}-character Git object ID",
            format.hex_len()
        ));
    }
    Ok(())
}

fn validate_new_records(existing: &[RevMapRecord], records: &[RevMapRecord]) -> Result<(), String> {
    let mut last = existing.last().map(|record| record.revision);
    for record in records {
        if last.is_some_and(|revision| record.revision <= revision) {
            return Err(format!(
                "out-of-order import rev_map record for revision {}",
                record.revision
            ));
        }
        last = Some(record.revision);
    }
    Ok(())
}

fn validate_recovery_prefix(
    journal: &ImportJournal,
    existing: &[RevMapRecord],
) -> Result<(), String> {
    if (journal.original_record_count == 0) != journal.original_tail.is_none() {
        return Err("import journal has inconsistent original rev_map state".to_string());
    }
    if existing.len() < journal.original_record_count {
        return Err("import rev_map was truncated after the journal was written".to_string());
    }
    if journal.original_record_count > 0
        && existing.get(journal.original_record_count - 1) != journal.original_tail.as_ref()
    {
        return Err("import rev_map original tail does not match the journal".to_string());
    }
    let appended = &existing[journal.original_record_count..];
    if appended.len() > journal.publication.records.len()
        || appended
            .iter()
            .zip(&journal.publication.records)
            .any(|(actual, expected)| actual != expected)
    {
        return Err("import rev_map suffix does not match the journal".to_string());
    }
    Ok(())
}

fn existing_records(path: &Path, format: ObjectFormat) -> Result<Vec<RevMapRecord>, String> {
    if path.exists() {
        RevMap::open_existing(path, format)?.records()
    } else {
        Ok(Vec::new())
    }
}

fn current_ref_oid(git: &GitCli, refname: &str) -> Result<String, String> {
    if git.refs_under(refname)?.iter().any(|name| name == refname) {
        Ok(git.rev_parse(refname)?.trim().to_string())
    } else {
        Ok("0".repeat(git.object_format()?.hex_len()))
    }
}

fn svn_metadata_root(git: &GitCli) -> Result<PathBuf, String> {
    Ok(git.work_tree().join(git.git_dir()?).join("svn"))
}

fn journal_path(git: &GitCli) -> Result<PathBuf, String> {
    Ok(svn_metadata_root(git)?.join(JOURNAL_FILE))
}

fn save(path: &Path, journal: &ImportJournal) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "import journal has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| error.to_string())?;
        file.write_all(encode(journal).as_bytes())
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temp, path).map_err(|error| error.to_string())?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn encode(journal: &ImportJournal) -> String {
    let publication = &journal.publication;
    let append_path = publication
        .append
        .as_ref()
        .map(|append| escape(&append.path.to_string_lossy()))
        .unwrap_or_else(|| "-".to_string());
    let append_payload = publication
        .append
        .as_ref()
        .map(|append| hex::encode(&append.payload))
        .unwrap_or_else(|| "-".to_string());
    let mut output = format!(
        "{MAGIC}\nrefname\t{}\nexpected_old_oid\t{}\ntarget_oid\t{}\nrev_map_path\t{}\noriginal_record_count\t{}\noriginal_tail\t{}\nappend_path\t{}\nappend_original_len\t{}\nappend_original_sha256\t{}\nappend_payload_len\t{}\nappend_payload_sha256\t{}\nappend_payload\t{}\nrecord_count\t{}\n",
        escape(&publication.refname),
        publication.expected_old_oid,
        publication.target_oid,
        escape(&publication.rev_map_path.to_string_lossy()),
        journal.original_record_count,
        journal
            .original_tail
            .as_ref()
            .map(encode_record)
            .unwrap_or_else(|| "-".to_string()),
        append_path,
        journal.append_original_len,
        journal.append_original_sha256,
        journal.append_payload_len,
        journal.append_payload_sha256,
        append_payload,
        publication.records.len()
    );
    for record in &publication.records {
        output.push_str(&format!("record\t{}\n", encode_record(record)));
    }
    output
}

fn load(path: &Path) -> Result<Option<ImportJournal>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut lines = text.lines();
    let version = lines
        .next()
        .ok_or_else(|| "invalid import journal header".to_string())?;
    if version != MAGIC && version != MAGIC_V1 {
        return Err("invalid import journal header".to_string());
    }
    let refname = unescape(field(&mut lines, "refname")?)?;
    let expected_old_oid = field(&mut lines, "expected_old_oid")?.to_string();
    let target_oid = field(&mut lines, "target_oid")?.to_string();
    let rev_map_path = PathBuf::from(unescape(field(&mut lines, "rev_map_path")?)?);
    let original_record_count = parse_usize(field(&mut lines, "original_record_count")?)?;
    let tail = field(&mut lines, "original_tail")?;
    let original_tail = if tail == "-" {
        None
    } else {
        Some(decode_record(tail)?)
    };
    let (
        append,
        append_original_len,
        append_original_sha256,
        append_payload_len,
        append_payload_sha256,
    ) = if version == MAGIC {
        let append_path = field(&mut lines, "append_path")?;
        let append_original_len = parse_usize(field(&mut lines, "append_original_len")?)?;
        let append_original_sha256 = field(&mut lines, "append_original_sha256")?.to_string();
        let append_payload_len = parse_usize(field(&mut lines, "append_payload_len")?)?;
        let append_payload_sha256 = field(&mut lines, "append_payload_sha256")?.to_string();
        let append_payload = field(&mut lines, "append_payload")?;
        let append = match (append_path, append_payload) {
            ("-", "-") => None,
            ("-", _) | (_, "-") => {
                return Err("import journal has incomplete append fields".to_string());
            }
            (path, payload) => Some(ImportAppend {
                path: PathBuf::from(unescape(path)?),
                payload: hex::decode(payload).map_err(|error| error.to_string())?,
            }),
        };
        (
            append,
            append_original_len,
            append_original_sha256,
            append_payload_len,
            append_payload_sha256,
        )
    } else {
        (None, 0, sha256(&[]), 0, sha256(&[]))
    };
    let record_count = parse_usize(field(&mut lines, "record_count")?)?;
    let mut records = Vec::with_capacity(record_count);
    for _ in 0..record_count {
        records.push(decode_record(field(&mut lines, "record")?)?);
    }
    if lines.next().is_some() {
        return Err("import journal contains trailing data".to_string());
    }
    Ok(Some(ImportJournal {
        publication: ImportPublication {
            refname,
            expected_old_oid,
            target_oid,
            rev_map_path,
            records,
            append,
        },
        original_record_count,
        original_tail,
        append_original_len,
        append_original_sha256,
        append_payload_len,
        append_payload_sha256,
    }))
}

fn field<'a>(lines: &mut impl Iterator<Item = &'a str>, name: &str) -> Result<&'a str, String> {
    let line = lines
        .next()
        .ok_or_else(|| format!("import journal is missing {name}"))?;
    line.strip_prefix(&format!("{name}\t"))
        .ok_or_else(|| format!("invalid import journal field {name}"))
}

fn parse_usize(value: &str) -> Result<usize, String> {
    value
        .parse()
        .map_err(|_| "invalid import journal count".to_string())
}

fn encode_record(record: &RevMapRecord) -> String {
    format!("{}:{}", record.revision, record.object_id_hex)
}

fn decode_record(value: &str) -> Result<RevMapRecord, String> {
    let (revision, oid) = value
        .split_once(':')
        .ok_or_else(|| "invalid import journal record".to_string())?;
    Ok(RevMapRecord {
        revision: revision
            .parse()
            .map_err(|_| "invalid import journal revision".to_string())?,
        object_id_hex: oid.to_string(),
    })
}

fn escape(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.:/\\".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn unescape(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut output = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err("invalid import journal escape".to_string());
            }
            output.push(
                u8::from_str_radix(&value[index + 1..index + 3], 16)
                    .map_err(|_| "invalid import journal escape".to_string())?,
            );
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).map_err(|_| "import journal is not valid UTF-8".to_string())
}

fn remove(path: &Path) -> Result<(), String> {
    fs::remove_file(path).map_err(|error| error.to_string())?;
    sync_directory(
        path.parent()
            .ok_or_else(|| "import journal has no parent".to_string())?,
    )
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), String> {
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    struct Fixture {
        _temp: TempDir,
        git: GitCli,
        refname: String,
        rev_map: PathBuf,
        old: String,
        target: String,
        other: String,
    }

    fn fixture() -> Fixture {
        let temp = TempDir::new().unwrap();
        let git = GitCli::new(temp.path());
        git.init().unwrap();
        git.config_set("user.name", "Import Test").unwrap();
        git.config_set("user.email", "import@example.com").unwrap();
        git.run_for_test(["commit", "--allow-empty", "-m", "old"])
            .unwrap();
        let old = git.rev_parse("HEAD").unwrap().trim().to_string();
        git.run_for_test(["commit", "--allow-empty", "-m", "target"])
            .unwrap();
        let target = git.rev_parse("HEAD").unwrap().trim().to_string();
        git.run_for_test(["commit", "--allow-empty", "-m", "other"])
            .unwrap();
        let other = git.rev_parse("HEAD").unwrap().trim().to_string();
        let refname = "refs/remotes/origin/trunk".to_string();
        git.update_ref(&refname, &old).unwrap();
        let rev_map = svn_metadata_root(&git)
            .unwrap()
            .join("origin.trunk")
            .join(".rev_map.uuid");
        Fixture {
            _temp: temp,
            git,
            refname,
            rev_map,
            old,
            target,
            other,
        }
    }

    fn publication(f: &Fixture) -> ImportPublication {
        ImportPublication {
            refname: f.refname.clone(),
            expected_old_oid: f.old.clone(),
            target_oid: f.target.clone(),
            rev_map_path: f.rev_map.clone(),
            records: vec![RevMapRecord {
                revision: 1,
                object_id_hex: f.target.clone(),
            }],
            append: None,
        }
    }

    fn journal_for(f: &Fixture) -> ImportJournal {
        ImportJournal {
            publication: publication(f),
            original_record_count: 0,
            original_tail: None,
            append_original_len: 0,
            append_original_sha256: sha256(&[]),
            append_payload_len: 0,
            append_payload_sha256: sha256(&[]),
        }
    }

    fn journal_with_append(f: &Fixture, original: &[u8], payload: &[u8]) -> ImportJournal {
        let mut publication = publication(f);
        publication.append = Some(ImportAppend {
            path: f.rev_map.parent().unwrap().join("unhandled.log"),
            payload: payload.to_vec(),
        });
        ImportJournal {
            publication,
            original_record_count: 0,
            original_tail: None,
            append_original_len: original.len(),
            append_original_sha256: sha256(original),
            append_payload_len: payload.len(),
            append_payload_sha256: sha256(payload),
        }
    }

    #[test]
    fn recovers_when_ref_is_still_old() {
        let f = fixture();
        save(&journal_path(&f.git).unwrap(), &journal_for(&f)).unwrap();
        assert!(recover_pending(&GitCli::new(f.git.work_tree())).unwrap());
        assert_eq!(current_ref_oid(&f.git, &f.refname).unwrap(), f.target);
        assert_eq!(
            RevMap::open_existing(&f.rev_map, ObjectFormat::Sha1)
                .unwrap()
                .get(1)
                .unwrap(),
            Some(f.target)
        );
    }

    #[test]
    fn recovers_when_ref_is_already_target_and_map_is_partial() {
        let f = fixture();
        let mut journal = journal_for(&f);
        journal.publication.records.push(RevMapRecord {
            revision: 2,
            object_id_hex: f.other.clone(),
        });
        journal.publication.target_oid = f.other.clone();
        save(&journal_path(&f.git).unwrap(), &journal).unwrap();
        f.git
            .update_ref_expected(&f.refname, &f.other, &f.old)
            .unwrap();
        let mut map = RevMap::open(&f.rev_map, ObjectFormat::Sha1).unwrap();
        map.append(1, &f.target).unwrap();
        assert!(recover_pending(&GitCli::new(f.git.work_tree())).unwrap());
        assert_eq!(
            RevMap::open_existing(&f.rev_map, ObjectFormat::Sha1)
                .unwrap()
                .get(2)
                .unwrap(),
            Some(f.other)
        );
    }

    #[test]
    fn cas_conflict_preserves_journal_and_rev_map() {
        let f = fixture();
        save(&journal_path(&f.git).unwrap(), &journal_for(&f)).unwrap();
        f.git.update_ref(&f.refname, &f.other).unwrap();
        let error = recover_pending(&f.git).unwrap_err();
        assert!(error.contains("moved to"));
        assert!(journal_path(&f.git).unwrap().exists());
        assert!(!f.rev_map.exists());
    }

    #[test]
    fn completed_recovery_is_idempotent() {
        let f = fixture();
        complete(&f.git, publication(&f)).unwrap();
        assert!(!recover_pending(&GitCli::new(f.git.work_tree())).unwrap());
        let records = RevMap::open_existing(&f.rev_map, ObjectFormat::Sha1)
            .unwrap()
            .records()
            .unwrap();
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn completes_ref_map_and_append_together() {
        let f = fixture();
        let log = f.rev_map.parent().unwrap().join("unhandled.log");
        fs::create_dir_all(log.parent().unwrap()).unwrap();
        fs::write(&log, b"original\n").unwrap();
        let mut publication = publication(&f);
        publication.append = Some(ImportAppend {
            path: log.clone(),
            payload: b"r1\n  +file_prop: trunk/a custom:value x\n".to_vec(),
        });

        complete(&f.git, publication).unwrap();

        assert_eq!(
            fs::read(&log).unwrap(),
            b"original\nr1\n  +file_prop: trunk/a custom:value x\n"
        );
        assert!(!journal_path(&f.git).unwrap().exists());
    }

    #[test]
    fn recovers_a_partially_appended_payload() {
        let f = fixture();
        let original = b"original\n";
        let payload = b"r1\nmetadata\n";
        let journal = journal_with_append(&f, original, payload);
        let log = journal.publication.append.as_ref().unwrap().path.clone();
        fs::create_dir_all(log.parent().unwrap()).unwrap();
        fs::write(&log, [original.as_slice(), &payload[..5]].concat()).unwrap();
        save(&journal_path(&f.git).unwrap(), &journal).unwrap();
        f.git
            .update_ref_expected(&f.refname, &f.target, &f.old)
            .unwrap();
        let mut map = RevMap::open(&f.rev_map, ObjectFormat::Sha1).unwrap();
        map.append(1, &f.target).unwrap();

        assert!(recover_pending(&f.git).unwrap());
        assert_eq!(
            fs::read(log).unwrap(),
            [original.as_slice(), payload].concat()
        );
        assert!(!journal_path(&f.git).unwrap().exists());
    }

    #[test]
    fn append_conflict_is_detected_before_ref_or_map_mutation() {
        let f = fixture();
        let journal = journal_with_append(&f, b"original\n", b"r1\nmetadata\n");
        let log = journal.publication.append.as_ref().unwrap().path.clone();
        fs::create_dir_all(log.parent().unwrap()).unwrap();
        fs::write(&log, b"changed!\n").unwrap();
        save(&journal_path(&f.git).unwrap(), &journal).unwrap();

        let error = recover_pending(&f.git).unwrap_err();

        assert!(error.contains("prefix does not match"));
        assert_eq!(current_ref_oid(&f.git, &f.refname).unwrap(), f.old);
        assert!(!f.rev_map.exists());
        assert!(journal_path(&f.git).unwrap().exists());
    }
}
