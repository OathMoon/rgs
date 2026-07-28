use std::path::{Path, PathBuf};

pub fn svn_metadata_dir(git_dir: &Path, refname: &str) -> Result<PathBuf, String> {
    let canonical = git_dir.join("svn").join(refname);
    let legacy = legacy_svn_metadata_dir(git_dir, refname);
    if canonical != legacy && canonical.exists() && legacy.exists() {
        return Err(format!(
            "ambiguous git-svn metadata for {refname}: both {} and {} exist",
            canonical.display(),
            legacy.display()
        ));
    }
    if canonical.exists() || !legacy.exists() {
        Ok(canonical)
    } else {
        Ok(legacy)
    }
}

pub fn legacy_svn_metadata_dir(git_dir: &Path, refname: &str) -> PathBuf {
    let short_ref = refname
        .strip_prefix("refs/remotes/")
        .unwrap_or(refname)
        .replace('/', ".");
    git_dir.join("svn").join(short_ref)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_metadata_uses_the_canonical_full_ref_path() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            svn_metadata_dir(temp.path(), "refs/remotes/origin/topic").unwrap(),
            temp.path().join("svn/refs/remotes/origin/topic")
        );
    }

    #[test]
    fn existing_flattened_metadata_remains_readable() {
        let temp = tempfile::tempdir().unwrap();
        let legacy = temp.path().join("svn/origin.topic");
        std::fs::create_dir_all(&legacy).unwrap();
        assert_eq!(
            svn_metadata_dir(temp.path(), "refs/remotes/origin/topic").unwrap(),
            legacy
        );
    }

    #[test]
    fn canonical_and_legacy_metadata_are_not_silently_mixed() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("svn/refs/remotes/origin/topic")).unwrap();
        std::fs::create_dir_all(temp.path().join("svn/origin.topic")).unwrap();
        let error = svn_metadata_dir(temp.path(), "refs/remotes/origin/topic").unwrap_err();
        assert!(error.contains("ambiguous git-svn metadata"));
    }
}
