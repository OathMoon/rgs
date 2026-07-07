use std::path::Path;

use crate::cli::GcArgs;
use crate::git::GitCli;
use flate2::Compression;
use flate2::write::GzEncoder;
use std::io::Write;

pub fn run(_args: GcArgs) -> Result<(), String> {
    run_in_work_tree(".")
}

pub fn run_in_work_tree(work_tree: impl Into<std::path::PathBuf>) -> Result<(), String> {
    let git = GitCli::new(work_tree.into());
    let git_dir = git.git_dir()?;
    let svn_dir = git.work_tree().join(git_dir).join("svn");
    if !svn_dir.exists() {
        return Ok(());
    }
    clean_svn_metadata(&svn_dir)
}

fn clean_svn_metadata(path: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            clean_svn_metadata(&path)?;
            continue;
        }

        let name = path.file_name().and_then(|name| name.to_str());
        if name == Some("unhandled.log") {
            compress_unhandled_log(&path)?;
        } else if name == Some("index")
            || name.is_some_and(|name| name.starts_with(".rev_map.") && name.ends_with(".lock"))
        {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn compress_unhandled_log(path: &Path) -> Result<(), String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&data).map_err(|e| e.to_string())?;
    let compressed = encoder.finish().map_err(|e| e.to_string())?;
    std::fs::write(path.with_file_name("unhandled.log.gz"), compressed)
        .map_err(|e| e.to_string())?;
    std::fs::remove_file(path).map_err(|e| e.to_string())
}
