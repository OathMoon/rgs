use crate::git::GitCli;
use crate::rev_map::RevMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const MAGIC: &str = "git-svn-rs-reset-journal-v1";
const JOURNAL_FILE: &str = "reset-journal";

struct ResetJournal {
    refname: String,
    expected_old_oid: String,
    target_oid: String,
    target_revision: u32,
    rev_map_path: PathBuf,
}

pub fn ensure_no_pending(git: &GitCli) -> Result<(), String> {
    let path = journal_path(git)?;
    if path.exists() {
        Err(format!(
            "unfinished reset journal found at {}; run reset again to recover it",
            path.display()
        ))
    } else {
        Ok(())
    }
}

pub fn recover_pending(git: &GitCli) -> Result<(), String> {
    let path = journal_path(git)?;
    let Some(journal) = load(&path)? else {
        return Ok(());
    };
    let metadata_root = path
        .parent()
        .ok_or_else(|| "reset journal has no metadata root".to_string())?
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let canonical_rev_map = journal
        .rev_map_path
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !canonical_rev_map.starts_with(&metadata_root) {
        return Err(format!(
            "reset journal rev_map path escapes SVN metadata root: {}",
            journal.rev_map_path.display()
        ));
    }
    let current = git.rev_parse(&journal.refname)?.trim().to_string();
    if current == journal.expected_old_oid {
        git.update_ref_expected(
            &journal.refname,
            &journal.target_oid,
            &journal.expected_old_oid,
        )?;
    } else if current != journal.target_oid {
        return Err(format!(
            "cannot recover reset: {} moved to {current}, expected {} or {}",
            journal.refname, journal.expected_old_oid, journal.target_oid
        ));
    }
    RevMap::open(&canonical_rev_map, git.object_format()?)?
        .reset_to(journal.target_revision, &journal.target_oid)?;
    remove(&path)
}

pub fn execute(
    git: &GitCli,
    refname: &str,
    rev_map_path: &Path,
    target_revision: u32,
    target_oid: &str,
) -> Result<(), String> {
    let path = journal_path(git)?;
    let journal = ResetJournal {
        refname: refname.to_string(),
        expected_old_oid: git.rev_parse(refname)?.trim().to_string(),
        target_oid: target_oid.to_string(),
        target_revision,
        rev_map_path: rev_map_path.to_path_buf(),
    };
    save(&path, &journal)?;
    if let Err(error) = git.update_ref_expected(
        &journal.refname,
        &journal.target_oid,
        &journal.expected_old_oid,
    ) {
        remove(&path)?;
        return Err(error);
    }
    RevMap::open(&journal.rev_map_path, git.object_format()?)?
        .reset_to(journal.target_revision, &journal.target_oid)?;
    remove(&path)
}

fn journal_path(git: &GitCli) -> Result<PathBuf, String> {
    Ok(git
        .work_tree()
        .join(git.git_dir()?)
        .join("svn")
        .join(JOURNAL_FILE))
}

fn save(path: &Path, journal: &ResetJournal) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "reset journal has no parent directory".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let bytes = format!(
        "{MAGIC}\nrefname\t{}\nexpected_old_oid\t{}\ntarget_oid\t{}\ntarget_revision\t{}\nrev_map_path\t{}\n",
        encode(&journal.refname),
        journal.expected_old_oid,
        journal.target_oid,
        journal.target_revision,
        encode(&journal.rev_map_path.to_string_lossy()),
    );
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| error.to_string())?;
        file.write_all(bytes.as_bytes())
            .map_err(|error| error.to_string())?;
        file.flush().map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temp, path).map_err(|error| error.to_string())?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

fn load(path: &Path) -> Result<Option<ResetJournal>, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let mut lines = text.lines();
    if lines.next() != Some(MAGIC) {
        return Err("invalid reset journal header".to_string());
    }
    let refname = field(&mut lines, "refname").and_then(decode)?;
    let expected_old_oid = field(&mut lines, "expected_old_oid")?.to_string();
    let target_oid = field(&mut lines, "target_oid")?.to_string();
    let target_revision = field(&mut lines, "target_revision")?
        .parse::<u32>()
        .map_err(|_| "invalid reset journal target revision".to_string())?;
    let rev_map_path = PathBuf::from(field(&mut lines, "rev_map_path").and_then(decode)?);
    if lines.next().is_some() {
        return Err("reset journal contains trailing data".to_string());
    }
    Ok(Some(ResetJournal {
        refname,
        expected_old_oid,
        target_oid,
        target_revision,
        rev_map_path,
    }))
}

fn field<'a>(lines: &mut impl Iterator<Item = &'a str>, name: &str) -> Result<&'a str, String> {
    let line = lines
        .next()
        .ok_or_else(|| format!("reset journal is missing {name}"))?;
    line.strip_prefix(&format!("{name}\t"))
        .ok_or_else(|| format!("invalid reset journal field {name}"))
}

fn encode(value: &str) -> String {
    hex::encode(value.as_bytes())
}

fn decode(value: &str) -> Result<String, String> {
    String::from_utf8(hex::decode(value).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

fn remove(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => sync_directory(
            path.parent()
                .ok_or_else(|| "reset journal has no parent directory".to_string())?,
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), String> {
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rev_map::ObjectFormat;

    fn commit(git: &GitCli, path: &str, content: &str, message: &str) -> String {
        fs::write(git.work_tree().join(path), content).unwrap();
        git.run_for_test(["add", path]).unwrap();
        git.run_for_test(["commit", "-m", message]).unwrap();
        git.rev_parse("HEAD").unwrap().trim().to_string()
    }

    fn fixture() -> (tempfile::TempDir, GitCli, PathBuf, String, String) {
        let temp = tempfile::tempdir().unwrap();
        let git = GitCli::new(temp.path());
        git.init().unwrap();
        git.config_set("user.name", "Test User").unwrap();
        git.config_set("user.email", "test@example.com").unwrap();
        let first = commit(&git, "one.txt", "one\n", "one");
        let second = commit(&git, "two.txt", "two\n", "two");
        git.update_ref("refs/remotes/git-svn", &second).unwrap();
        let rev_map_path = temp.path().join(".git/svn/git-svn/.rev_map.test-uuid");
        let mut rev_map = RevMap::open(&rev_map_path, ObjectFormat::Sha1).unwrap();
        rev_map.append(1, &first).unwrap();
        rev_map.append(2, &second).unwrap();
        (temp, git, rev_map_path, first, second)
    }

    fn pending(git: &GitCli, rev_map_path: &Path, target_oid: &str, expected_old_oid: &str) {
        save(
            &journal_path(git).unwrap(),
            &ResetJournal {
                refname: "refs/remotes/git-svn".to_string(),
                expected_old_oid: expected_old_oid.to_string(),
                target_oid: target_oid.to_string(),
                target_revision: 1,
                rev_map_path: rev_map_path.to_path_buf(),
            },
        )
        .unwrap();
    }

    #[test]
    fn recovers_journal_before_ref_update() {
        let (_temp, git, rev_map_path, first, second) = fixture();
        pending(&git, &rev_map_path, &first, &second);
        assert!(ensure_no_pending(&git).is_err());

        recover_pending(&git).unwrap();

        assert_eq!(git.rev_parse("refs/remotes/git-svn").unwrap().trim(), first);
        assert_eq!(
            RevMap::open(rev_map_path, ObjectFormat::Sha1)
                .unwrap()
                .get(2)
                .unwrap(),
            None
        );
        ensure_no_pending(&git).unwrap();
    }

    #[test]
    fn recovers_journal_after_ref_update() {
        let (_temp, git, rev_map_path, first, second) = fixture();
        pending(&git, &rev_map_path, &first, &second);
        git.update_ref_expected("refs/remotes/git-svn", &first, &second)
            .unwrap();

        recover_pending(&git).unwrap();

        assert_eq!(git.rev_parse("refs/remotes/git-svn").unwrap().trim(), first);
        assert_eq!(
            RevMap::open(rev_map_path, ObjectFormat::Sha1)
                .unwrap()
                .max_revision(true)
                .unwrap(),
            Some(1)
        );
        ensure_no_pending(&git).unwrap();
    }

    #[test]
    fn recovery_rejects_rev_map_outside_svn_metadata() {
        let (temp, git, _rev_map_path, first, second) = fixture();
        let outside = temp.path().join("outside.rev_map");
        let mut map = RevMap::open(&outside, ObjectFormat::Sha1).unwrap();
        map.append(1, &first).unwrap();
        map.append(2, &second).unwrap();
        pending(&git, &outside, &first, &second);

        let error = recover_pending(&git).unwrap_err();

        assert!(error.contains("escapes SVN metadata root"));
        assert_eq!(
            git.rev_parse("refs/remotes/git-svn").unwrap().trim(),
            second
        );
        assert!(ensure_no_pending(&git).is_err());
    }
}
