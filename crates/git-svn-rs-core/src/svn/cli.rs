use std::collections::BTreeMap;
use std::num::ParseIntError;
use std::process::Command;

use crate::config::SvnRemoteConfig;
use crate::svn::editor::FetchEditor;
use crate::svn::ra::{DirEntry, DirListing, RaSession, SvnNodeKind, UpdateRequest};
use crate::svn::{ChangeAction, ChangedPath, NodeKind, RevisionEvent, SvnBackend};

#[derive(Debug, Clone)]
pub struct SvnCliBackend {
    url: String,
    username: Option<String>,
    password: Option<String>,
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
            password: None,
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

    #[cfg(test)]
    pub(crate) fn configured_username(&self) -> Option<&str> {
        self.username.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn configured_config_dir(&self) -> Option<&str> {
        self.config_dir.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn configured_password(&self) -> Option<&str> {
        self.password.as_deref()
    }

    pub fn with_password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
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

    pub fn repository_root(&self) -> Result<String, String> {
        Ok(self
            .run_text(&["info", "--show-item", "repos-root-url", &self.url])?
            .trim()
            .to_string())
    }

    fn command_args(&self, args: &[&str]) -> Vec<String> {
        let mut command_args = vec!["--non-interactive".to_string()];
        if let Some(config_dir) = &self.config_dir {
            command_args.push("--config-dir".to_string());
            command_args.push(config_dir.clone());
        }
        if let Some(username) = &self.username {
            command_args.push("--username".to_string());
            command_args.push(username.clone());
        }
        if let Some(password) = &self.password {
            command_args.push("--password".to_string());
            command_args.push(password.clone());
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

    fn cat(&self, repos_root: &str, path: &str, revision: u32) -> Result<Vec<u8>, String> {
        let url = versioned_url(repos_root, path, revision);
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
        repos_root: &str,
        path: &str,
        revision: u32,
    ) -> Result<BTreeMap<String, String>, String> {
        self.node_properties(repos_root, path, revision)
    }

    fn node_properties(
        &self,
        repos_root: &str,
        path: &str,
        revision: u32,
    ) -> Result<BTreeMap<String, String>, String> {
        let url = versioned_url(repos_root, path, revision);
        let xml = self.run_text(&[
            "proplist",
            "--xml",
            "--verbose",
            "--depth",
            "empty",
            "--non-interactive",
            "-r",
            &revision.to_string(),
            &url,
        ])?;
        Ok(parse_proplist_xml_bytes(&xml)?
            .into_iter()
            .filter_map(|(name, value)| String::from_utf8(value).ok().map(|value| (name, value)))
            .collect())
    }

    fn node_property_bytes(
        &self,
        repos_root: &str,
        path: &str,
        revision: u32,
    ) -> Result<BTreeMap<String, Vec<u8>>, String> {
        let url = versioned_url(repos_root, path, revision);
        let xml = self.run_text(&[
            "proplist",
            "--xml",
            "--verbose",
            "--depth",
            "empty",
            "--non-interactive",
            "-r",
            &revision.to_string(),
            &url,
        ])?;
        parse_proplist_xml_bytes(&xml)
    }

    fn list_files(
        &self,
        repos_root: &str,
        path: &str,
        revision: u32,
    ) -> Result<Vec<String>, String> {
        let url = versioned_url(repos_root, path, revision);
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
}

fn versioned_url(repos_root: &str, path: &str, revision: u32) -> String {
    format!(
        "{}/{}@{}",
        repos_root.trim_end_matches('/'),
        path.trim_start_matches('/'),
        revision
    )
}

fn is_svn_cli_supported_url(url: &str) -> bool {
    matches!(
        crate::path_url::svn_url_profile(url),
        crate::path_url::SvnUrlProfile::File
            | crate::path_url::SvnUrlProfile::Svn
            | crate::path_url::SvnUrlProfile::Http
            | crate::path_url::SvnUrlProfile::Https
            | crate::path_url::SvnUrlProfile::SvnSsh
    )
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
        let repos_root = self.repository_root()?;
        let session_path = self
            .run_text(&["info", "--show-item", "relative-url", &self.url])?
            .trim()
            .strip_prefix("^/")
            .unwrap_or_default()
            .trim_matches('/')
            .to_string();
        let mut revisions = parse_log_xml(&xml)?;
        for revision in &mut revisions {
            let mut copied_files = Vec::new();
            for path in &mut revision.changed_paths {
                if matches!(
                    path.action,
                    ChangeAction::Add | ChangeAction::Modify | ChangeAction::Replace
                ) && path.kind == NodeKind::File
                {
                    path.content = Some(self.cat(&repos_root, &path.path, revision.revision)?);
                    path.properties =
                        self.file_properties(&repos_root, &path.path, revision.revision)?;
                }
                if matches!(
                    path.action,
                    ChangeAction::Add | ChangeAction::Modify | ChangeAction::Replace
                ) && path.kind == NodeKind::Directory
                {
                    path.properties =
                        self.node_properties(&repos_root, &path.path, revision.revision)?;
                }
                if matches!(path.action, ChangeAction::Add | ChangeAction::Replace)
                    && path.kind == NodeKind::Directory
                    && path.copy_from_path.is_some()
                {
                    for relative in self.list_files(&repos_root, &path.path, revision.revision)? {
                        let file_path = format!("{}/{}", path.path.trim_end_matches('/'), relative);
                        copied_files.push(ChangedPath {
                            path: file_path.clone(),
                            action: ChangeAction::Add,
                            copy_from_path: path.copy_from_path.as_ref().map(|source| {
                                format!("{}/{}", source.trim_end_matches('/'), relative)
                            }),
                            copy_from_rev: path.copy_from_rev,
                            kind: NodeKind::File,
                            properties_modified: true,
                            content_modified: true,
                            properties: self.file_properties(
                                &repos_root,
                                &file_path,
                                revision.revision,
                            )?,
                            content: Some(self.cat(&repos_root, &file_path, revision.revision)?),
                        });
                    }
                }
            }
            revision.changed_paths.extend(copied_files);
        }
        normalize_changed_paths(&mut revisions, &session_path);
        Ok(revisions)
    }
}

impl RaSession for SvnCliBackend {
    fn url(&self) -> &str {
        &self.url
    }

    fn repos_root(&self) -> &str {
        &self.url
    }

    fn uuid(&self) -> Result<String, String> {
        SvnBackend::uuid(self)
    }

    fn latest_revnum(&self) -> Result<u32, String> {
        SvnBackend::latest_revnum(self)
    }

    fn check_path(&self, path: &str, revision: u32) -> Result<Option<SvnNodeKind>, String> {
        let url = versioned_url(&self.url, path, revision);
        match self.run_text(&["info", "--show-item", "kind", &url]) {
            Ok(kind) => Ok(match kind.trim() {
                "file" => Some(SvnNodeKind::File),
                "dir" => Some(SvnNodeKind::Directory),
                _ => None,
            }),
            Err(error) if error.contains("not found") || error.contains("does not exist") => {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    fn get_dir(&self, path: &str, revision: u32) -> Result<DirListing, String> {
        let mut entries = BTreeMap::new();
        for file in self.list_files(&self.url, path, revision)? {
            entries.insert(
                file,
                DirEntry {
                    kind: SvnNodeKind::File,
                },
            );
        }
        Ok(DirListing {
            entries,
            properties: BTreeMap::new(),
        })
    }

    fn get_log(&self, paths: &[&str], start: u32, end: u32) -> Result<Vec<RevisionEvent>, String> {
        let mut revisions = SvnBackend::log(self, start, end)?;
        if !paths.is_empty() {
            revisions.retain(|revision| {
                revision.changed_paths.iter().any(|changed_path| {
                    paths.iter().any(|path| {
                        path_contains(path, &changed_path.path)
                            || path_contains(&changed_path.path, path)
                    })
                })
            });
        }
        Ok(revisions)
    }

    fn do_update(
        &self,
        path: &str,
        revision: u32,
        editor: &mut dyn FetchEditor,
    ) -> Result<(), String> {
        self.do_update_from(
            path,
            UpdateRequest {
                target_revision: revision,
                base_revision: None,
            },
            editor,
        )
    }

    fn do_update_from(
        &self,
        path: &str,
        request: UpdateRequest,
        editor: &mut dyn FetchEditor,
    ) -> Result<(), String> {
        let revision = SvnBackend::log(self, request.target_revision, request.target_revision)?
            .into_iter()
            .find(|revision| revision.revision == request.target_revision)
            .ok_or_else(|| format!("SVN revision r{} was not found", request.target_revision))?;
        editor.open_root(request.target_revision)?;
        for changed_path in revision
            .changed_paths
            .iter()
            .filter(|changed_path| path_contains(path, &changed_path.path))
        {
            let properties = if changed_path.action == ChangeAction::Delete {
                BTreeMap::new()
            } else {
                self.node_property_bytes(&self.url, &changed_path.path, request.target_revision)?
            };
            let removed_properties = if changed_path.action == ChangeAction::Modify {
                request
                    .base_revision
                    .map(|base_revision| {
                        self.node_property_bytes(&self.url, &changed_path.path, base_revision)
                    })
                    .transpose()?
                    .unwrap_or_default()
                    .into_keys()
                    .filter(|name| !properties.contains_key(name))
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            drive_changed_path(
                editor,
                changed_path,
                request.base_revision,
                &properties,
                &removed_properties,
            )?;
        }
        editor.close_edit()
    }

    fn do_switch(
        &self,
        path: &str,
        revision: u32,
        _switch_url: &str,
        editor: &mut dyn FetchEditor,
    ) -> Result<(), String> {
        self.do_update(path, revision, editor)
    }
}

fn path_contains(parent: &str, candidate: &str) -> bool {
    let parent = parent.trim_matches('/');
    let candidate = candidate.trim_matches('/');
    parent.is_empty() || candidate == parent || candidate.starts_with(&format!("{parent}/"))
}

fn drive_changed_path(
    editor: &mut dyn FetchEditor,
    changed_path: &ChangedPath,
    base_revision: Option<u32>,
    properties: &BTreeMap<String, Vec<u8>>,
    removed_properties: &[String],
) -> Result<(), String> {
    let path = changed_path.path.trim_matches('/');
    if matches!(
        changed_path.action,
        ChangeAction::Delete | ChangeAction::Replace
    ) {
        editor.delete_entry(path, base_revision.unwrap_or_default())?;
    }
    if changed_path.action == ChangeAction::Delete {
        return Ok(());
    }

    match changed_path.kind {
        NodeKind::Directory => {
            if matches!(
                changed_path.action,
                ChangeAction::Add | ChangeAction::Replace
            ) {
                // `SvnBackend::log` expands copied directories and loads full
                // file contents, so the CLI adapter replays a self-contained
                // snapshot delta. Copy ancestry still comes from the log event.
                editor.add_directory(path, None)?;
            }
            for (name, value) in properties {
                editor.change_directory_prop_bytes(path, name, Some(value))?;
            }
            for name in removed_properties {
                editor.change_directory_prop(path, name, None)?;
            }
        }
        NodeKind::File | NodeKind::Symlink => {
            if matches!(
                changed_path.action,
                ChangeAction::Add | ChangeAction::Replace
            ) {
                editor.add_file(path, None)?;
            }
            for (name, value) in properties {
                editor.change_file_prop_bytes(path, name, Some(value))?;
            }
            for name in removed_properties {
                editor.change_file_prop(path, name, None)?;
            }
            if let Some(content) = &changed_path.content {
                editor.apply_textdelta(path, content)?;
            }
        }
    }
    Ok(())
}

fn parse_proplist_xml_bytes(xml: &str) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut properties = BTreeMap::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<property") {
        rest = &rest[start..];
        let header_end = rest
            .find('>')
            .ok_or_else(|| "invalid svn proplist XML: unterminated property".to_string())?;
        let header = &rest[..=header_end];
        let name = attr(header, "name")
            .ok_or_else(|| "invalid svn proplist XML: property without name".to_string())?;
        if header.ends_with("/>") {
            properties.insert(name, Vec::new());
            rest = &rest[header_end + 1..];
            continue;
        }
        let value_start = header_end + 1;
        let value_end = rest[value_start..]
            .find("</property>")
            .ok_or_else(|| "invalid svn proplist XML: unclosed property".to_string())?
            + value_start;
        let raw_value = &rest[value_start..value_end];
        let value = match attr(header, "encoding").as_deref() {
            None => xml_unescape(raw_value).into_bytes(),
            Some("base64") => decode_base64(raw_value)
                .map_err(|error| format!("invalid base64 SVN property {name}: {error}"))?,
            Some(encoding) => {
                return Err(format!(
                    "unsupported SVN property encoding {encoding} for {name}"
                ));
            }
        };
        properties.insert(name, value);
        rest = &rest[value_end + "</property>".len()..];
    }
    Ok(properties)
}

fn decode_base64(value: &str) -> Result<Vec<u8>, String> {
    let input = value
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if input.len() % 4 != 0 {
        return Err("length is not a multiple of four".to_string());
    }
    let mut decoded = Vec::with_capacity(input.len() / 4 * 3);
    for (index, chunk) in input.chunks_exact(4).enumerate() {
        let last = index + 1 == input.len() / 4;
        let a = base64_digit(chunk[0])?;
        let b = base64_digit(chunk[1])?;
        let c = if chunk[2] == b'=' {
            if chunk[3] != b'=' || !last {
                return Err("invalid padding".to_string());
            }
            0
        } else {
            base64_digit(chunk[2])?
        };
        let d = if chunk[3] == b'=' {
            if !last {
                return Err("invalid padding".to_string());
            }
            0
        } else {
            base64_digit(chunk[3])?
        };
        decoded.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            decoded.push((b << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            decoded.push((c << 6) | d);
        }
    }
    Ok(decoded)
}

fn base64_digit(byte: u8) -> Result<u8, String> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(format!("invalid character 0x{byte:02X}")),
    }
}

fn normalize_changed_paths(revisions: &mut [RevisionEvent], session_path: &str) {
    if session_path.is_empty() {
        return;
    }
    for revision in revisions {
        revision.changed_paths.retain_mut(|changed_path| {
            let Some(path) = session_relative_path(&changed_path.path, session_path) else {
                return false;
            };
            changed_path.path = path;
            if let Some(copy_from_path) = &changed_path.copy_from_path {
                changed_path.copy_from_path = session_relative_path(copy_from_path, session_path);
            }
            true
        });
    }
}

fn session_relative_path(path: &str, session_path: &str) -> Option<String> {
    let path = path.trim_matches('/');
    if path == session_path {
        return Some(String::new());
    }
    path.strip_prefix(&format!("{session_path}/"))
        .map(str::to_string)
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
            properties_modified: true,
            content_modified: true,
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
            .with_password("secret")
            .with_config_dir("svn-config")
            .without_auth_cache();

        assert_eq!(
            backend.command_args(&["info", "file:///repo"]),
            vec![
                "--non-interactive",
                "--config-dir",
                "svn-config",
                "--username",
                "alice",
                "--password",
                "secret",
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
            "SVN+SSH://svn.example/repo",
        ] {
            SvnCliBackend::new(url).unwrap();
        }
    }

    #[test]
    fn versioned_urls_use_repository_root_for_subdirectory_sessions() {
        assert_eq!(
            versioned_url("file:///repo", "/trunk/src/lib.rs", 2),
            "file:///repo/trunk/src/lib.rs@2"
        );
    }

    #[test]
    fn changed_paths_are_normalized_to_the_session_root() {
        assert_eq!(
            session_relative_path("/trunk/src/lib.rs", "trunk"),
            Some("src/lib.rs".to_string())
        );
        assert_eq!(session_relative_path("/branches/topic/file", "trunk"), None);
    }

    #[test]
    fn parses_verbose_proplist_xml_for_unknown_and_empty_properties() {
        let properties = parse_proplist_xml_bytes(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<properties>
<target path="file:///repo/trunk">
<property name="custom:message">hello &amp; goodbye</property>
<property name="custom:empty"/>
</target>
</properties>"#,
        )
        .unwrap();

        assert_eq!(
            properties.get("custom:message").unwrap(),
            b"hello & goodbye"
        );
        assert_eq!(properties.get("custom:empty").unwrap(), b"");
    }

    #[test]
    fn decodes_encoded_binary_properties_as_bytes() {
        let properties = parse_proplist_xml_bytes(
            r#"<properties><target path="x"><property name="custom:binary" encoding="base64">AP+A</property></target></properties>"#,
        )
        .unwrap();

        assert_eq!(properties.get("custom:binary").unwrap(), &[0, 0xff, 0x80]);
    }
}
