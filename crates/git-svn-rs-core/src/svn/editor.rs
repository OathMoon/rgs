pub trait FetchEditor {
    fn open_root(&mut self, revision: u32) -> Result<(), String>;
    fn add_directory(&mut self, path: &str, copy_from: Option<(&str, u32)>) -> Result<(), String>;
    fn add_file(&mut self, path: &str, copy_from: Option<(&str, u32)>) -> Result<(), String>;
    fn delete_entry(&mut self, path: &str, revision: u32) -> Result<(), String>;
    fn change_file_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String>;
    fn change_directory_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        let _ = (path, name, value);
        Ok(())
    }
    fn absent_directory(&mut self, path: &str) -> Result<(), String> {
        let _ = path;
        Ok(())
    }
    fn absent_file(&mut self, path: &str) -> Result<(), String> {
        let _ = path;
        Ok(())
    }
    fn apply_textdelta(&mut self, path: &str, content: &[u8]) -> Result<(), String>;
    fn close_edit(&mut self) -> Result<(), String>;
    fn abort_edit(&mut self) -> Result<(), String> {
        Ok(())
    }
}

pub trait CommitEditor {
    fn ensure_path(&mut self, path: &str) -> Result<(), String>;
    fn add_file(&mut self, path: &str, content: &[u8]) -> Result<(), String>;
    fn open_file(&mut self, path: &str, content: &[u8]) -> Result<(), String>;
    fn delete_entry(&mut self, path: &str) -> Result<(), String>;
    fn copy_file(
        &mut self,
        source_path: &str,
        source_revision: u32,
        path: &str,
    ) -> Result<(), String> {
        Err(format!(
            "commit editor does not support copying {source_path}@{source_revision} to {path}"
        ))
    }
    fn move_entry(
        &mut self,
        source_path: &str,
        source_revision: u32,
        path: &str,
    ) -> Result<(), String> {
        Err(format!(
            "commit editor does not support moving {source_path}@{source_revision} to {path}"
        ))
    }
    fn change_file_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String>;
    fn change_directory_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        Err(format!(
            "commit editor does not support changing directory property {name} on {path} to {value:?}"
        ))
    }
    fn close_edit(&mut self) -> Result<u32, String>;
    fn abort_edit(&mut self) -> Result<(), String>;
}
