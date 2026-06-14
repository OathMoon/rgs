use std::path::{Path, PathBuf};

pub fn svn_metadata_dir(git_dir: &Path, refname: &str) -> PathBuf {
    git_dir.join("svn").join(refname)
}
