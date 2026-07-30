use std::path::{Path, PathBuf};

pub(crate) fn walk_files_no_symlinks(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let root_type = std::fs::symlink_metadata(root)
        .map_err(|error| error.to_string())?
        .file_type();
    if root_type.is_symlink() {
        return Ok(files);
    }
    walk(root, &mut files)?;
    Ok(files)
}

fn walk(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(root).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        let file_type = std::fs::symlink_metadata(&path)
            .map_err(|error| error.to_string())?
            .file_type();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            walk(&path, files)?;
        } else if file_type.is_file() {
            files.push(path);
        }
    }
    Ok(())
}
