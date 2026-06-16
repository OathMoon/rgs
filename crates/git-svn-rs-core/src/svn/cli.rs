use std::collections::BTreeMap;
use std::num::ParseIntError;
use std::process::Command;

use crate::config::SvnRemoteConfig;
use crate::svn::{ChangeAction, ChangedPath, NodeKind, RevisionEvent, SvnBackend};

#[derive(Debug, Clone)]
pub struct SvnCliBackend {
    url: String,
    username: Option<String>,
    config_dir: Option<String>,
    no_auth_cache: bool,
}

impl SvnCliBackend {
    pub fn new(url: impl Into<String>) -> Result<Self, String> {
        let url = url.into();
        if !is_svn_cli_supported_url(&url) {
            return Err(format!(
                "real SVN fetch via svn CLI does not support URL scheme in {url}"
            ));
        }
        Ok(Self {
            url,
            username: None,
            config_dir: None,
            no_auth_cache: false,
        })
    }

    pub fn from_config(config: &SvnRemoteConfig) -> Result<Self, String> {
        let mut backend = Self::new(&config.url)?;
        backend.username.clone_from(&config.username);
        backend.config_dir.clone_from(&config.config_dir);
        backend.no_auth_cache = config.no_auth_cache;
        Ok(backend)
    }

    pub fn with_username(mut self, username: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self
    }

    pub fn with_config_dir(mut self, config_dir: impl Into<String>) -> Self {
        self.config_dir = Some(config_dir.into());
        self
    }

    pub fn without_auth_cache(mut self) -> Self {
        self.no_auth_cache = true;
        self
    }

    fn command_args(&self, args: &[&str]) -> Vec<String> {
        let mut command_args = Vec::new();
        if let Some(config_dir) = &self.config_dir {
            command_args.push("--config-dir".to_string());
            command_args.push(config_dir.clone());
        }
        if let Some(username) = &self.username {
            command_args.push("--username".to_string());
            command_args.push(username.clone());
        }
        if self.no_auth_cache {
            command_args.push("--no-auth-cache".to_string());
        }
        command_args.extend(args.iter().map(|arg| (*arg).to_string()));
        command_args
    }

    fn run(&self, args: &[&str]) -> Result<Vec<u8>, String> {
        let command_args = self.command_args(args);
        let output = Command::new("svn")
            .args(command_args)
            .output()
            .map_err(|e| format!("svn failed to start: {e}"))?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(if stderr.is_empty() {
                format!("svn exited with status {}", output.status)
            } else {
                stderr
            })
        }
    }

    fn run_text(&self, args: &[&str]) -> Result<String, String> {
        String::from_utf8(self.run(args)?).map_err(|e| e.to_string())
    }

    fn cat(&self, path: &str, revision: u32) -> Result<Vec<u8>, String> {
        let url = self.versioned_url(path, revision);
        self.run(&[
            "cat",
            "--non-interactive",
            "-r",
            &revision.to_string(),
            &url,
        ])
    }

    fn file_properties(
        &self,
        path: &str,
        revision: u32,
    ) -> Result<BTreeMap<String, String>, String> {
        let mut properties = BTreeMap::new();
        let url = self.versioned_url(path, revision);
        for name in ["svn:executable", "svn:special"] {
            let value = match self.run_text(&[
                "propget",
                "--strict",
                "--non-interactive",
                "-r",
                &revision.to_string(),
                name,
                &url,
            ]) {
                Ok(value) => value,
                Err(error) if error.contains(&format!("Property '{name}' not found")) => {
                    String::new()
                }
                Err(error) => return Err(error),
            };
            if !value.is_empty() {
                properties.insert(name.to_string(), value);
            }
        }
        Ok(properties)
    }

    fn list_files(&self, path: &str, revision: u32) -> Result<Vec<String>, String> {
        let url = self.versioned_url(path, revision);
        let output = self.run_text(&[
            "list",
            "--recursive",
            "--non-interactive",
            "-r",
            &revision.to_string(),
            &url,
        ])?;
        Ok(output
            .lines()
            .filter(|line| !line.ends_with('/'))
            .map(|line| line.trim_matches('/').to_string())
            .filter(|line| !line.is_empty())
            .collect())
    }

    fn versioned_url(&self, path: &str, revision: u32) -> String {
        format!(
            "{}/{}@{}",
            self.url.trim_end_matches('/'),
            path.trim_start_matches('/'),
            revision
        )
    }
}

fn is_svn_cli_supported_url(url: &str) -> bool {
    ["file://", "http://", "https://", "svn://", "svn+ssh://"]
        .iter()
        .any(|prefix| url.starts_with(prefix))
}

impl SvnBackend for SvnCliBackend {
    fn uuid(&self) -> Result<String, String> {
        Ok(self
            .run_text(&["info", "--show-item", "repos-uuid", &self.url])?
            .trim()
            .to_string())
    }

    fn latest_revnum(&self) -> Result<u32, String> {
        self.run_text(&["info", "--show-item", "revision", &self.url])?
            .trim()
            .parse()
            .map_err(|e: ParseIntError| e.to_string())
    }

    fn log(&self, start: u32, end: u32) -> Result<Vec<RevisionEvent>, String> {
        let range = format!("{start}:{end}");
        let xml = self.run_text(&["log", "--xml", "-v", "-r", &range, &self.url])?;
        let mut revisions = parse_log_xml(&xml)?;
        for revision in &mut revisions {
            let mut copied_files = Vec::new();
            for path in &mut revision.changed_paths {
                if matches!(
                    path.action,
                    ChangeAction::Add | ChangeAction::Modify | ChangeAction::Replace
                ) && path.kind == NodeKind::File
                {
                    path.content = Some(self.cat(&path.path, revision.revision)?);
                    path.properties = self.file_properties(&path.path, revision.revision)?;
                }
                if matches!(path.action, ChangeAction::Add | ChangeAction::Replace)
                    && path.kind == NodeKind::Directory
                    && path.copy_from_path.is_some()
                {
                    for relative in self.list_files(&path.path, revision.revision)? {
                        let file_path = format!("{}/{}", path.path.trim_end_matches('/'), relative);
                        copied_files.push(ChangedPath {
                            path: file_path.clone(),
                            action: ChangeAction::Add,
                            copy_from_path: path.copy_from_path.as_ref().map(|source| {
                                format!("{}/{}", source.trim_end_matches('/'), relative)
                            }),
                            copy_from_rev: path.copy_from_rev,
                            kind: NodeKind::File,
                            properties: self.file_properties(&file_path, revision.revision)?,
                            content: Some(self.cat(&file_path, revision.revision)?),
                        });
                    }
                }
            }
            revision.changed_paths.extend(copied_files);
        }
        Ok(revisions)
    }
}

fn parse_log_xml(xml: &str) -> Result<Vec<RevisionEvent>, String> {
    let mut revisions = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<logentry") {
        rest = &rest[start..];
        let header_end = rest
            .find('>')
            .ok_or_else(|| "invalid svn log xml: unterminated logentry".to_string())?;
        let header = &rest[..=header_end];
        let revision = attr(header, "revision")
            .ok_or_else(|| "invalid svn log xml: logentry without revision".to_string())?
            .parse()
            .map_err(|e: ParseIntError| e.to_string())?;
        let body_start = header_end + 1;
        let body_end = rest[body_start..]
            .find("</logentry>")
            .ok_or_else(|| "invalid svn log xml: unclosed logentry".to_string())?
            + body_start;
        let body = &rest[body_start..body_end];

        revisions.push(RevisionEvent {
            revision,
            author: element_text(body, "author").unwrap_or_default(),
            message: element_text(body, "msg").unwrap_or_default(),
            timestamp: element_text(body, "date").unwrap_or_default(),
            changed_paths: parse_changed_paths(body)?,
        });
        rest = &rest[body_end + "</logentry>".len()..];
    }
    Ok(revisions)
}

fn parse_changed_paths(body: &str) -> Result<Vec<ChangedPath>, String> {
    let Some(paths_start) = body.find("<paths>") else {
        return Ok(Vec::new());
    };
    let paths_body_start = paths_start + "<paths>".len();
    let paths_end = body[paths_body_start..]
        .find("</paths>")
        .ok_or_else(|| "invalid svn log xml: unclosed paths".to_string())?
        + paths_body_start;
    let mut paths = Vec::new();
    let mut rest = &body[paths_body_start..paths_end];

    while let Some(start) = rest.find("<path") {
        rest = &rest[start..];
        let header_end = rest
            .find('>')
            .ok_or_else(|| "invalid svn log xml: unterminated path".to_string())?;
        let header = &rest[..=header_end];
        let text_end = rest[header_end + 1..]
            .find("</path>")
            .ok_or_else(|| "invalid svn log xml: unclosed path".to_string())?
            + header_end
            + 1;
        let text = &rest[header_end + 1..text_end];

        paths.push(ChangedPath {
            path: xml_unescape(text.trim()),
            action: parse_action(attr(header, "action").as_deref())?,
            copy_from_path: attr(header, "copyfrom-path"),
            copy_from_rev: attr(header, "copyfrom-rev")
                .map(|value| value.parse().map_err(|e: ParseIntError| e.to_string()))
                .transpose()?,
            kind: parse_kind(attr(header, "kind").as_deref()),
            properties: BTreeMap::new(),
            content: None,
        });
        rest = &rest[text_end + "</path>".len()..];
    }

    Ok(paths)
}

fn element_text(body: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = body.find(&open)? + open.len();
    let end = body[start..].find(&close)? + start;
    Some(xml_unescape(&body[start..end]))
}

fn attr(header: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = header.find(&needle)? + needle.len();
    let end = header[start..].find('"')? + start;
    Some(xml_unescape(&header[start..end]))
}

fn parse_action(action: Option<&str>) -> Result<ChangeAction, String> {
    match action {
        Some("A") => Ok(ChangeAction::Add),
        Some("M") => Ok(ChangeAction::Modify),
        Some("D") => Ok(ChangeAction::Delete),
        Some("R") => Ok(ChangeAction::Replace),
        Some(other) => Err(format!("unsupported svn path action: {other}")),
        None => Err("svn path is missing action".to_string()),
    }
}

fn parse_kind(kind: Option<&str>) -> NodeKind {
    match kind {
        Some("file") => NodeKind::File,
        Some("dir") => NodeKind::Directory,
        _ => NodeKind::Directory,
    }
}

fn xml_unescape(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_deleted_file_path_action() {
        let paths = parse_changed_paths(
            r#"
<paths>
<path
   action="D"
   prop-mods="false"
   text-mods="false"
   kind="file">/trunk/deleted.txt</path>
</paths>
"#,
        )
        .unwrap();

        assert_eq!(paths[0].path, "/trunk/deleted.txt");
        assert_eq!(paths[0].action, ChangeAction::Delete);
        assert_eq!(paths[0].kind, NodeKind::File);
    }

    #[test]
    fn backend_command_args_include_auth_and_config_options() {
        let backend = SvnCliBackend::new("file:///repo")
            .unwrap()
            .with_username("alice")
            .with_config_dir("svn-config")
            .without_auth_cache();

        assert_eq!(
            backend.command_args(&["info", "file:///repo"]),
            vec![
                "--config-dir",
                "svn-config",
                "--username",
                "alice",
                "--no-auth-cache",
                "info",
                "file:///repo",
            ]
        );
    }

    #[test]
    fn backend_accepts_svn_cli_supported_remote_url_schemes() {
        for url in [
            "file:///repo",
            "http://svn.example/repo",
            "https://svn.example/repo",
            "svn://svn.example/repo",
            "svn+ssh://svn.example/repo",
        ] {
            SvnCliBackend::new(url).unwrap();
        }
    }
}
