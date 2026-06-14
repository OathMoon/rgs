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
    fn apply_textdelta(&mut self, path: &str, content: &[u8]) -> Result<(), String>;
    fn close_edit(&mut self) -> Result<(), String>;
}

pub trait CommitEditor {
    fn ensure_path(&mut self, path: &str) -> Result<(), String>;
    fn add_file(&mut self, path: &str, content: &[u8]) -> Result<(), String>;
    fn open_file(&mut self, path: &str, content: &[u8]) -> Result<(), String>;
    fn delete_entry(&mut self, path: &str) -> Result<(), String>;
    fn change_file_prop(
        &mut self,
        path: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String>;
    fn close_edit(&mut self) -> Result<u32, String>;
    fn abort_edit(&mut self) -> Result<(), String>;
}
