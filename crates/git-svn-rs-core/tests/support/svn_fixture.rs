use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;

use tempfile::TempDir;

const MISSING_TOOLS: &str = "svnadmin and svn are required";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SvnToolPolicy {
    Skip(String),
    Fail(String),
}

pub fn has_svn_tools() -> bool {
    command_succeeds("svnadmin", &["--version"]) && command_succeeds("svn", &["--version"])
}

pub fn require_svn_tools() -> Result<(), SvnToolPolicy> {
    if has_svn_tools() {
        Ok(())
    } else {
        Err(missing_tools_policy(strict_compat()))
    }
}

pub fn missing_tools_policy(strict_compat: bool) -> SvnToolPolicy {
    if strict_compat {
        SvnToolPolicy::Fail(MISSING_TOOLS.to_string())
    } else {
        SvnToolPolicy::Skip(format!("skipping: {MISSING_TOOLS}"))
    }
}

#[allow(dead_code)]
pub fn require_svnserve() -> Result<(), SvnToolPolicy> {
    if command_succeeds("svnserve", &["--version"]) {
        Ok(())
    } else if strict_compat() {
        Err(SvnToolPolicy::Fail("svnserve is required".to_string()))
    } else {
        Err(SvnToolPolicy::Skip(
            "skipping: svnserve is required".to_string(),
        ))
    }
}

#[cfg(unix)]
#[allow(dead_code)]
pub fn require_http_dav() -> Result<(), SvnToolPolicy> {
    if http_dav_tools(false).is_some() {
        Ok(())
    } else if strict_compat() {
        Err(SvnToolPolicy::Fail(
            "apache2/httpd, htpasswd, and mod_dav_svn are required".to_string(),
        ))
    } else {
        Err(SvnToolPolicy::Skip(
            "skipping: apache2/httpd, htpasswd, and mod_dav_svn are required".to_string(),
        ))
    }
}

#[cfg(unix)]
#[allow(dead_code)]
pub fn require_https_dav() -> Result<(), SvnToolPolicy> {
    if http_dav_tools(true).is_some() && command_succeeds("openssl", &["version"]) {
        Ok(())
    } else if strict_compat() {
        Err(SvnToolPolicy::Fail(
            "Apache DAV SVN with mod_ssl and openssl is required".to_string(),
        ))
    } else {
        Err(SvnToolPolicy::Skip(
            "skipping: Apache DAV SVN with mod_ssl and openssl is required".to_string(),
        ))
    }
}

pub struct StandardSvnFixture {
    _tmp: TempDir,
    repo: PathBuf,
}

impl StandardSvnFixture {
    pub fn create() -> Result<Self, String> {
        let tmp = tempfile::Builder::new()
            .prefix("svn-fixture-")
            .tempdir_in(std::env::current_dir().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let repo = tmp.path().join("repo");
        let wc = tmp.path().join("wc");

        run(tmp.path(), "svnadmin", &["create", path_arg(&repo)?])?;

        let url = file_url(&repo)?;
        run(
            tmp.path(),
            "svn",
            &[
                "checkout",
                "--non-interactive",
                url.as_str(),
                path_arg(&wc)?,
            ],
        )?;
        run(
            &wc,
            "svn",
            &["mkdir", "--non-interactive", "trunk", "branches", "tags"],
        )?;
        run(&wc, "svn", &["commit", "--non-interactive", "-m", "layout"])?;

        std::fs::create_dir_all(wc.join("trunk/src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(wc.join("trunk/empty-dir")).map_err(|e| e.to_string())?;
        std::fs::write(
            wc.join("trunk/src/lib.rs"),
            "pub fn answer() -> u8 { 42 }\n",
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(wc.join("trunk/run.sh"), "#!/bin/sh\necho hi\n")
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        std::os::unix::fs::symlink("src/lib.rs", wc.join("trunk/link-to-lib"))
            .map_err(|e| e.to_string())?;
        #[cfg(not(unix))]
        std::fs::write(wc.join("trunk/link-to-lib"), "link src/lib.rs")
            .map_err(|e| e.to_string())?;
        run(
            &wc,
            "svn",
            &[
                "add",
                "--non-interactive",
                "trunk/src",
                "trunk/run.sh",
                "trunk/link-to-lib",
                "trunk/empty-dir",
            ],
        )?;
        run(
            &wc,
            "svn",
            &[
                "propset",
                "--non-interactive",
                "svn:executable",
                "x",
                "trunk/run.sh",
            ],
        )?;
        #[cfg(not(unix))]
        run(
            &wc,
            "svn",
            &[
                "propset",
                "--non-interactive",
                "svn:special",
                "x",
                "trunk/link-to-lib",
            ],
        )?;
        run(
            &wc,
            "svn",
            &["commit", "--non-interactive", "-m", "add trunk file"],
        )?;

        run(
            &wc,
            "svn",
            &["copy", "--non-interactive", "trunk", "branches/main"],
        )?;
        run(
            &wc,
            "svn",
            &["commit", "--non-interactive", "-m", "branch main"],
        )?;

        run(
            &wc,
            "svn",
            &["copy", "--non-interactive", "trunk", "tags/v1"],
        )?;
        run(&wc, "svn", &["commit", "--non-interactive", "-m", "tag v1"])?;

        Ok(Self { _tmp: tmp, repo })
    }

    pub fn url(&self) -> String {
        file_url(&self.repo).expect("fixture repository path should convert to file URL")
    }

    #[allow(dead_code)]
    pub fn root(&self) -> &Path {
        self.repo
            .parent()
            .expect("fixture repository should have a parent")
    }

    pub fn latest_revision(&self) -> u32 {
        let output = Command::new("svn")
            .args(["info", "--show-item", "revision", self.url().as_str()])
            .output()
            .expect("svn info should run");
        assert!(
            output.status.success(),
            "svn info failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .expect("svn info revision should be numeric")
    }

    #[allow(dead_code)]
    pub fn set_revision_property(
        &self,
        revision: u32,
        name: &str,
        value: &[u8],
    ) -> Result<(), String> {
        let value_path = self._tmp.path().join("revision-property-value");
        std::fs::write(&value_path, value).map_err(|error| error.to_string())?;
        run(
            self._tmp.path(),
            "svnadmin",
            &[
                "setrevprop",
                path_arg(&self.repo)?,
                "-r",
                &revision.to_string(),
                name,
                path_arg(&value_path)?,
            ],
        )
    }

    #[allow(dead_code)]
    pub fn create_peg_sensitive_trunk(&self) -> Result<u32, String> {
        let wc = self._tmp.path().join("create-peg-sensitive-trunk-wc");
        run(
            self._tmp.path(),
            "svn",
            &[
                "checkout",
                "--non-interactive",
                self.url().as_str(),
                path_arg(&wc)?,
            ],
        )?;
        std::fs::create_dir(wc.join("trunk@main")).map_err(|e| e.to_string())?;
        std::fs::write(wc.join("trunk@main/run.sh"), "#!/bin/sh\necho hi\n")
            .map_err(|e| e.to_string())?;
        run(&wc, "svn", &["add", "--non-interactive", "trunk@main@"])?;
        run(
            &wc,
            "svn",
            &[
                "commit",
                "--non-interactive",
                "-m",
                "add peg-sensitive trunk",
            ],
        )?;
        Ok(self.latest_revision())
    }

    #[allow(dead_code)]
    pub fn modify_peg_sensitive_run_script(&self, content: &str) -> Result<u32, String> {
        let wc = self._tmp.path().join("modify-peg-sensitive-trunk-wc");
        let target = format!("{}/trunk%40main@", self.url());
        run(
            self._tmp.path(),
            "svn",
            &[
                "checkout",
                "--non-interactive",
                target.as_str(),
                path_arg(&wc)?,
            ],
        )?;
        std::fs::write(wc.join("run.sh"), content).map_err(|e| e.to_string())?;
        run(
            &wc,
            "svn",
            &[
                "commit",
                "--non-interactive",
                "-m",
                "modify peg-sensitive run script",
            ],
        )?;
        Ok(self.latest_revision())
    }

    #[allow(dead_code)]
    pub fn remove_executable_from_run_script(&self) -> Result<u32, String> {
        self.remove_run_script_property("svn:executable", "remove executable property")
    }

    #[allow(dead_code)]
    pub fn modify_run_script_content(&self, content: &str) -> Result<u32, String> {
        let wc = self._tmp.path().join("modify-run-script-content-wc");
        run(
            self._tmp.path(),
            "svn",
            &[
                "checkout",
                "--non-interactive",
                self.url().as_str(),
                path_arg(&wc)?,
            ],
        )?;
        std::fs::write(wc.join("trunk/run.sh"), content).map_err(|e| e.to_string())?;
        run(
            &wc,
            "svn",
            &[
                "commit",
                "--non-interactive",
                "-m",
                "modify run script content",
            ],
        )?;
        Ok(self.latest_revision())
    }

    #[allow(dead_code)]
    pub fn remove_run_script_property(&self, name: &str, message: &str) -> Result<u32, String> {
        let wc = self._tmp.path().join("remove-executable-wc");
        run(
            self._tmp.path(),
            "svn",
            &[
                "checkout",
                "--non-interactive",
                self.url().as_str(),
                path_arg(&wc)?,
            ],
        )?;
        run(
            &wc,
            "svn",
            &["propdel", "--non-interactive", name, "trunk/run.sh"],
        )?;
        run(&wc, "svn", &["commit", "--non-interactive", "-m", message])?;
        Ok(self.latest_revision())
    }

    #[allow(dead_code)]
    pub fn set_trunk_dir_property(&self, name: &str, value: &str) -> Result<u32, String> {
        let wc = self._tmp.path().join("set-trunk-dir-property-wc");
        run(
            self._tmp.path(),
            "svn",
            &[
                "checkout",
                "--non-interactive",
                self.url().as_str(),
                path_arg(&wc)?,
            ],
        )?;
        run(
            &wc,
            "svn",
            &["propset", "--non-interactive", name, value, "trunk"],
        )?;
        run(
            &wc,
            "svn",
            &[
                "commit",
                "--non-interactive",
                "-m",
                "set trunk dir property",
            ],
        )?;
        Ok(self.latest_revision())
    }

    #[allow(dead_code)]
    pub fn set_run_script_property(&self, name: &str, value: &str) -> Result<u32, String> {
        let wc = self._tmp.path().join("set-run-script-property-wc");
        run(
            self._tmp.path(),
            "svn",
            &[
                "checkout",
                "--non-interactive",
                self.url().as_str(),
                path_arg(&wc)?,
            ],
        )?;
        run(
            &wc,
            "svn",
            &["propset", "--non-interactive", name, value, "trunk/run.sh"],
        )?;
        run(
            &wc,
            "svn",
            &[
                "commit",
                "--non-interactive",
                "-m",
                "set run script property",
            ],
        )?;
        Ok(self.latest_revision())
    }

    #[allow(dead_code)]
    pub fn uuid(&self) -> String {
        let output = Command::new("svn")
            .args(["info", "--show-item", "repos-uuid", self.url().as_str()])
            .output()
            .expect("svn info should run");
        assert!(
            output.status.success(),
            "svn info failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    #[allow(dead_code)]
    pub fn require_basic_auth(&self, username: &str, password: &str) -> Result<(), String> {
        let conf = self.repo.join("conf");
        std::fs::write(
            conf.join("svnserve.conf"),
            "[general]\nanon-access = none\nauth-access = read\npassword-db = passwd\n",
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(
            conf.join("passwd"),
            format!("[users]\n{username} = {password}\n"),
        )
        .map_err(|e| e.to_string())
    }

    #[allow(dead_code)]
    pub fn allow_anonymous_write(&self) -> Result<(), String> {
        std::fs::write(
            self.repo.join("conf").join("svnserve.conf"),
            "[general]\nanon-access = write\n",
        )
        .map_err(|e| e.to_string())
    }

    #[allow(dead_code)]
    pub fn require_write_auth(&self, username: &str, password: &str) -> Result<(), String> {
        let conf = self.repo.join("conf");
        std::fs::write(
            conf.join("svnserve.conf"),
            "[general]\nanon-access = read\nauth-access = write\npassword-db = passwd\n",
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(
            conf.join("passwd"),
            format!("[users]\n{username} = {password}\n"),
        )
        .map_err(|e| e.to_string())
    }

    #[allow(dead_code)]
    pub fn require_read_write_auth(&self, username: &str, password: &str) -> Result<(), String> {
        let conf = self.repo.join("conf");
        std::fs::write(
            conf.join("svnserve.conf"),
            "[general]\nanon-access = none\nauth-access = write\npassword-db = passwd\n",
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(
            conf.join("passwd"),
            format!("[users]\n{username} = {password}\n"),
        )
        .map_err(|e| e.to_string())
    }
}

#[allow(dead_code)]
pub struct SvnServe {
    child: Child,
    port: u16,
}

static SVNSERVE_START_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[allow(dead_code)]
impl SvnServe {
    pub fn start(root: &Path) -> Result<Self, String> {
        let _start_guard = SVNSERVE_START_LOCK
            .lock()
            .map_err(|_| "svnserve start lock is poisoned".to_string())?;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        drop(listener);

        let child = Command::new("svnserve")
            .args([
                "--daemon",
                "--foreground",
                "--listen-host",
                "127.0.0.1",
                "--listen-port",
                &port.to_string(),
                "--root",
                path_arg(root)?,
            ])
            .spawn()
            .map_err(|e| format!("svnserve failed to start: {e}"))?;
        let mut server = Self { child, port };
        server.wait_until_ready()?;
        Ok(server)
    }

    pub fn repo_url(&self) -> String {
        self.repository_url("repo")
    }

    pub fn repository_url(&self, name: &str) -> String {
        format!("svn://127.0.0.1:{}/{name}", self.port)
    }

    fn wait_until_ready(&mut self) -> Result<(), String> {
        let mut last_error = String::new();
        for _ in 0..50 {
            match std::net::TcpStream::connect(("127.0.0.1", self.port)) {
                Ok(_) => {
                    return Ok(());
                }
                Err(err) => {
                    last_error = err.to_string();
                }
            }
            if self.child.try_wait().map_err(|e| e.to_string())?.is_some() {
                return Err("svnserve exited before accepting connections".to_string());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(format!("svnserve did not become ready: {last_error}"))
    }
}

impl Drop for SvnServe {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(unix)]
#[allow(dead_code)]
pub struct HttpDav {
    child: Child,
    _runtime: TempDir,
    port: u16,
    tls: bool,
    certificate: Option<PathBuf>,
}

#[cfg(unix)]
static HTTP_DAV_START_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(unix)]
#[allow(dead_code)]
impl HttpDav {
    pub fn start_basic(
        repository_root: &Path,
        username: &str,
        password: &str,
    ) -> Result<Self, String> {
        Self::start(repository_root, username, password, false)
    }

    pub fn start_basic_tls(
        repository_root: &Path,
        username: &str,
        password: &str,
    ) -> Result<Self, String> {
        Self::start(repository_root, username, password, true)
    }

    fn start(
        repository_root: &Path,
        username: &str,
        password: &str,
        tls: bool,
    ) -> Result<Self, String> {
        let (apache, module_dir) = http_dav_tools(tls)
            .ok_or_else(|| "Apache DAV SVN tools are unavailable".to_string())?;
        let _start_guard = HTTP_DAV_START_LOCK
            .lock()
            .map_err(|_| "HTTP DAV start lock is poisoned".to_string())?;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        drop(listener);

        let runtime = tempfile::Builder::new()
            .prefix("git-svn-rs-http-dav-")
            .tempdir()
            .map_err(|e| e.to_string())?;
        let runtime_path = apache_path(runtime.path())?;
        let repository_root = apache_path(repository_root)?;
        let password_file = runtime.path().join("htpasswd");
        let password_output = Command::new("htpasswd")
            .args(["-bcB"])
            .arg(&password_file)
            .arg(username)
            .arg(password)
            .output()
            .map_err(|e| format!("htpasswd failed to start: {e}"))?;
        if !password_output.status.success() {
            return Err("htpasswd failed to create the HTTP DAV credential file".to_string());
        }
        let password_file = apache_path(&password_file)?;
        let module_dir = apache_path(&module_dir)?;
        let (certificate, tls_modules, tls_config) = if tls {
            let certificate = runtime.path().join("certificate.pem");
            let private_key = runtime.path().join("private-key.pem");
            let certificate_output = Command::new("openssl")
                .args([
                    "req",
                    "-x509",
                    "-newkey",
                    "rsa:2048",
                    "-sha256",
                    "-nodes",
                    "-days",
                    "1",
                    "-subj",
                    "/CN=localhost",
                    "-addext",
                    "subjectAltName=DNS:localhost,IP:127.0.0.1",
                    "-addext",
                    "basicConstraints=critical,CA:TRUE",
                    "-keyout",
                ])
                .arg(&private_key)
                .arg("-out")
                .arg(&certificate)
                .output()
                .map_err(|error| format!("openssl failed to start: {error}"))?;
            if !certificate_output.status.success() {
                return Err("openssl failed to create the HTTPS DAV certificate".to_string());
            }
            let certificate_path = apache_path(&certificate)?;
            let private_key_path = apache_path(&private_key)?;
            (
                Some(certificate),
                format!(
                    "LoadModule socache_shmcb_module \"{module_dir}/mod_socache_shmcb.so\"\n\
                     LoadModule ssl_module \"{module_dir}/mod_ssl.so\"\n"
                ),
                format!(
                    "SSLEngine On\nSSLCertificateFile \"{certificate_path}\"\n\
                     SSLCertificateKeyFile \"{private_key_path}\"\n"
                ),
            )
        } else {
            (None, String::new(), String::new())
        };
        let config_path = runtime.path().join("httpd.conf");
        let config = format!(
            concat!(
                "ServerRoot \"{runtime_path}\"\n",
                "DefaultRuntimeDir \"{runtime_path}\"\n",
                "PidFile \"{runtime_path}/httpd.pid\"\n",
                "Listen 127.0.0.1:{port}\n",
                "ServerName 127.0.0.1\n",
                "LoadModule mpm_event_module \"{module_dir}/mod_mpm_event.so\"\n",
                "LoadModule authn_core_module \"{module_dir}/mod_authn_core.so\"\n",
                "LoadModule authn_file_module \"{module_dir}/mod_authn_file.so\"\n",
                "LoadModule authz_core_module \"{module_dir}/mod_authz_core.so\"\n",
                "LoadModule authz_user_module \"{module_dir}/mod_authz_user.so\"\n",
                "LoadModule auth_basic_module \"{module_dir}/mod_auth_basic.so\"\n",
                "LoadModule dav_module \"{module_dir}/mod_dav.so\"\n",
                "LoadModule dav_svn_module \"{module_dir}/mod_dav_svn.so\"\n",
                "LoadModule authz_svn_module \"{module_dir}/mod_authz_svn.so\"\n",
                "{tls_modules}",
                "ErrorLog \"{runtime_path}/error.log\"\n",
                "LogLevel warn\n",
                "{tls_config}",
                "<Location /svn>\n",
                "  DAV svn\n",
                "  SVNParentPath \"{repository_root}\"\n",
                "  SVNListParentPath On\n",
                "  AuthType Basic\n",
                "  AuthName \"git-svn-rs fixture\"\n",
                "  AuthUserFile \"{password_file}\"\n",
                "  Require valid-user\n",
                "</Location>\n",
            ),
            runtime_path = runtime_path,
            port = port,
            module_dir = module_dir,
            repository_root = repository_root,
            password_file = password_file,
            tls_modules = tls_modules,
            tls_config = tls_config,
        );
        std::fs::write(&config_path, config).map_err(|e| e.to_string())?;

        let child = Command::new(apache)
            .args(["-f", path_arg(&config_path)?, "-X"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("{apache} failed to start: {e}"))?;
        let mut server = Self {
            child,
            _runtime: runtime,
            port,
            tls,
            certificate,
        };
        server.wait_until_ready(username, password)?;
        Ok(server)
    }

    pub fn repo_url(&self) -> String {
        let scheme = if self.tls { "https" } else { "http" };
        format!("{scheme}://localhost:{}/svn/repo", self.port)
    }

    pub fn write_tls_config(&self, config_dir: &Path) -> Result<(), String> {
        let certificate = self
            .certificate
            .as_deref()
            .ok_or_else(|| "HTTP DAV fixture has no TLS certificate".to_string())?;
        std::fs::create_dir_all(config_dir).map_err(|error| error.to_string())?;
        std::fs::write(
            config_dir.join("servers"),
            format!(
                "[global]\nssl-authority-files = {}\n",
                path_arg(certificate)?
            ),
        )
        .map_err(|error| error.to_string())
    }

    fn wait_until_ready(&mut self, username: &str, password: &str) -> Result<(), String> {
        for _ in 0..50 {
            let ready = Command::new("svn")
                .args(["info", "--non-interactive", "--no-auth-cache"])
                .args(
                    self.tls
                        .then_some(["--trust-server-cert-failures", "unknown-ca"])
                        .into_iter()
                        .flatten(),
                )
                .args([
                    "--username",
                    username,
                    "--password",
                    password,
                    self.repo_url().as_str(),
                ])
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false);
            if ready {
                return Ok(());
            }
            if self.child.try_wait().map_err(|e| e.to_string())?.is_some() {
                let error_log = std::fs::read_to_string(self._runtime.path().join("error.log"))
                    .unwrap_or_default();
                return Err(format!(
                    "Apache DAV exited before accepting SVN requests: {}",
                    error_log.trim()
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err("Apache DAV did not become ready for SVN requests".to_string())
    }
}

#[cfg(unix)]
impl Drop for HttpDav {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn command_succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(unix)]
fn http_dav_tools(tls: bool) -> Option<(&'static str, PathBuf)> {
    let apache = ["apache2", "httpd"]
        .into_iter()
        .find(|program| Command::new(program).arg("-v").output().is_ok())?;
    Command::new("htpasswd").arg("-h").output().ok()?;
    let mut required_modules = vec![
        "mod_mpm_event.so",
        "mod_authn_core.so",
        "mod_authn_file.so",
        "mod_authz_core.so",
        "mod_authz_user.so",
        "mod_auth_basic.so",
        "mod_dav.so",
        "mod_dav_svn.so",
        "mod_authz_svn.so",
    ];
    if tls {
        required_modules.extend(["mod_socache_shmcb.so", "mod_ssl.so"]);
    }
    let configured_module_dir = std::env::var_os("GIT_SVN_RS_APACHE_MODULE_DIR").map(PathBuf::from);
    configured_module_dir
        .iter()
        .map(PathBuf::as_path)
        .chain([
            Path::new("/usr/lib/apache2/modules"),
            Path::new("/usr/lib64/httpd/modules"),
            Path::new("/usr/lib/httpd/modules"),
        ])
        .find(|directory| {
            required_modules
                .iter()
                .all(|module| directory.join(module).is_file())
        })
        .map(|directory| (apache, directory.to_path_buf()))
}

#[cfg(unix)]
fn apache_path(path: &Path) -> Result<String, String> {
    let path = path_arg(path)?;
    if path.contains(['"', '\n', '\r']) {
        return Err(format!(
            "path cannot be represented in Apache config: {path}"
        ));
    }
    Ok(path.to_string())
}

fn strict_compat() -> bool {
    std::env::var("GIT_SVN_RS_STRICT_COMPAT").as_deref() == Ok("1")
}

fn run(cwd: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(program)
        .current_dir(cwd)
        .args(args)
        .output()
        .map_err(|e| format!("{program} failed to start: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "{program} failed with status {}: {}{}",
            output.status,
            stderr.trim(),
            if stdout.trim().is_empty() {
                String::new()
            } else {
                format!(" stdout: {}", stdout.trim())
            }
        ))
    }
}

fn path_arg(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()))
}

fn file_url(path: &Path) -> Result<String, String> {
    let raw = path
        .canonicalize()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    let raw = raw.strip_prefix("//?/").unwrap_or(&raw);
    Ok(format!("file:///{}", raw.trim_start_matches('/')))
}

#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[derive(Debug)]
pub struct PtyCommandOutput {
    pub status: std::process::ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

#[cfg(target_os = "linux")]
#[allow(dead_code)]
pub fn run_with_pty_password(
    binary: &str,
    current_dir: &Path,
    args: &[&str],
    password: &str,
) -> PtyCommandOutput {
    run_with_pty(binary, current_dir, args, Some(password))
}

#[cfg(target_os = "linux")]
#[allow(dead_code)]
pub fn run_with_pty_without_prompt(
    binary: &str,
    current_dir: &Path,
    args: &[&str],
) -> PtyCommandOutput {
    run_with_pty(binary, current_dir, args, None)
}

#[cfg(target_os = "linux")]
fn run_with_pty(
    binary: &str,
    current_dir: &Path,
    args: &[&str],
    password: Option<&str>,
) -> PtyCommandOutput {
    use std::io::{Read, Write};
    use std::process::Stdio;
    use std::time::Duration;

    assert!(!binary.contains('\''));
    assert!(args.iter().all(|arg| !arg.contains('\'')));
    let command = std::iter::once(binary)
        .chain(args.iter().copied())
        .map(|arg| format!("'{arg}'"))
        .collect::<Vec<_>>()
        .join(" ");
    let mut child = Command::new("script")
        .current_dir(current_dir)
        .env_remove("GIT_ASKPASS")
        .env_remove("SSH_ASKPASS")
        .env("GIT_TERMINAL_PROMPT", "1")
        .args(["-qec", &command, "/dev/null"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut bytes = Vec::new();
    let mut byte = [0_u8; 1];
    let mut handled_through = 0;
    while stdout.read(&mut byte).unwrap() != 0 {
        bytes.push(byte[0]);
        if bytes[handled_through..].ends_with(b": ")
            && String::from_utf8_lossy(&bytes[handled_through..]).contains("Password for '")
        {
            let Some(password) = password else {
                child.kill().unwrap();
                panic!("terminal password prompt was unexpectedly observed");
            };
            std::thread::sleep(Duration::from_millis(50));
            writeln!(stdin, "{password}").unwrap();
            stdin.flush().unwrap();
            handled_through = bytes.len();
        }
    }
    drop(stdin);
    let status = child.wait().unwrap();
    let mut stderr = Vec::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_end(&mut stderr)
        .unwrap();
    assert_eq!(
        handled_through > 0,
        password.is_some(),
        "terminal password prompt expectation was not met"
    );
    PtyCommandOutput {
        status,
        stdout: String::from_utf8_lossy(&bytes).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    }
}
