#[derive(Debug, Clone, Default)]
pub struct PropertyMapper;

impl PropertyMapper {
    pub fn file_properties(
        &self,
        symlink: bool,
        executable: bool,
    ) -> Vec<(String, Option<String>)> {
        let mut properties = Vec::new();
        if symlink {
            properties.push(("svn:special".to_string(), Some("*".to_string())));
        }
        if executable {
            properties.push(("svn:executable".to_string(), Some("*".to_string())));
        }
        properties
    }
}
