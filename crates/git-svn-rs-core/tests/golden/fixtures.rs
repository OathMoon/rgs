use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PERL_GIT_SVN_REQUIRED: &str = "Perl git-svn is required";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompatDecision {
    Skip(String),
    Fail(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolAvailability {
    Available { version: String },
    Missing { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoldenFixtureStep {
    CreateStandardLayout,
    AddFile {
        path: &'static str,
        contents: &'static str,
    },
    Copy {
        from: &'static str,
        to: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoldenFixture {
    name: &'static str,
    steps: Vec<GoldenFixtureStep>,
}

impl GoldenFixture {
    pub fn standard_linear_history() -> Self {
        Self {
            name: "standard-linear-history",
            steps: vec![
                GoldenFixtureStep::CreateStandardLayout,
                GoldenFixtureStep::AddFile {
                    path: "trunk/src/lib.rs",
                    contents: "pub fn answer() -> u8 { 42 }\n",
                },
                GoldenFixtureStep::Copy {
                    from: "trunk",
                    to: "branches/main",
                },
                GoldenFixtureStep::Copy {
                    from: "trunk",
                    to: "tags/v1",
                },
            ],
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn steps(&self) -> &[GoldenFixtureStep] {
        &self.steps
    }
}

#[derive(Debug, Clone)]
pub struct GoldenArtifactCapture {
    root: PathBuf,
}

impl GoldenArtifactCapture {
    pub fn new(root: impl AsRef<Path>, case_name: &str) -> Result<Self, String> {
        let root = root.as_ref().join(case_name);
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        Ok(Self { root })
    }

    pub fn write_text(&self, relative_path: &str, contents: &str) -> Result<PathBuf, String> {
        let path = self.root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let mut normalized = contents.replace("\r\n", "\n").replace('\r', "\n");
        if !normalized.ends_with('\n') {
            normalized.push('\n');
        }
        fs::write(&path, normalized).map_err(|e| e.to_string())?;
        Ok(path)
    }
}

pub fn perl_git_svn_available() -> ToolAvailability {
    let output = Command::new("git").args(["svn", "--version"]).output();
    match output {
        Ok(output) if output.status.success() => ToolAvailability::Available {
            version: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        },
        Ok(output) => ToolAvailability::Missing {
            reason: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        },
        Err(error) => ToolAvailability::Missing {
            reason: error.to_string(),
        },
    }
}

pub fn missing_perl_git_svn_policy(strict_compat: bool) -> CompatDecision {
    if strict_compat {
        CompatDecision::Fail(PERL_GIT_SVN_REQUIRED.to_string())
    } else {
        CompatDecision::Skip(format!("skipping: {PERL_GIT_SVN_REQUIRED}"))
    }
}

pub fn require_perl_git_svn() -> Result<String, CompatDecision> {
    match perl_git_svn_available() {
        ToolAvailability::Available { version } => Ok(version),
        ToolAvailability::Missing { .. } => Err(missing_perl_git_svn_policy(strict_compat())),
    }
}

fn strict_compat() -> bool {
    std::env::var("GIT_SVN_RS_STRICT_COMPAT").as_deref() == Ok("1")
}
