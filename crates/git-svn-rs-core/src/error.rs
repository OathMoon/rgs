use std::error::Error as StdError;
use std::fmt;

use thiserror::Error;

use crate::dcommit::coordinator::CoordinatorError;
use crate::dcommit::journal::JournalError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Unsupported,
    Authentication,
    Ambiguity,
    MetadataCorruption,
    PartialWrite,
    ExternalCommand,
    InvalidInvocation,
    /// Transitional category for internal `Result<_, String>` paths that have
    /// not reached a typed boundary yet.
    Unclassified,
}

impl ErrorCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Authentication => "authentication",
            Self::Ambiguity => "ambiguity",
            Self::MetadataCorruption => "metadata-corruption",
            Self::PartialWrite => "partial-write",
            Self::ExternalCommand => "external-command",
            Self::InvalidInvocation => "invalid-invocation",
            Self::Unclassified => "unclassified",
        }
    }
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct GitSvnError {
    category: ErrorCategory,
    message: String,
    #[source]
    source: Option<Box<dyn StdError + Send + Sync + 'static>>,
}

impl GitSvnError {
    pub fn new(category: ErrorCategory, message: impl Into<String>) -> Self {
        Self {
            category,
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source<E>(category: ErrorCategory, source: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        Self {
            category,
            message: source.to_string(),
            source: Some(Box::new(source)),
        }
    }

    pub const fn category(&self) -> ErrorCategory {
        self.category
    }

    pub fn unsupported_command(command: impl fmt::Display) -> Self {
        Self::new(
            ErrorCategory::Unsupported,
            format!("unsupported in v1: {command}"),
        )
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Unsupported, message)
    }

    pub fn authentication(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Authentication, message)
    }

    pub fn ambiguity(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Ambiguity, message)
    }

    pub fn metadata_corruption(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::MetadataCorruption, message)
    }

    pub fn partial_write(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::PartialWrite, message)
    }

    pub fn external_command(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::ExternalCommand, message)
    }

    pub fn invalid_invocation(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::InvalidInvocation, message)
    }

    pub fn unclassified(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Unclassified, message)
    }

    /// Classifies legacy string errors only at the command boundary. New domain
    /// paths should construct a category directly and retain their source.
    pub fn from_command_error(message: impl Into<String>) -> Self {
        let message = message.into();
        let lower = message.to_ascii_lowercase();
        let category = if lower.starts_with("unsupported in v1:") {
            ErrorCategory::Unsupported
        } else if lower.contains("authentication")
            || lower.contains("authorization failed")
            || lower.contains("credentials")
            || lower.contains("password prompt")
            || lower.contains("e170001")
            || lower.contains("e215004")
        {
            ErrorCategory::Authentication
        } else if lower.contains("post-submit")
            || lower.contains("may have been submitted")
            || lower.contains("submission outcome is ambiguous")
            || lower.contains("submitted state could not be persisted")
        {
            ErrorCategory::PartialWrite
        } else if lower.contains("ambiguous")
            || lower.contains("matches multiple svn mappings")
            || lower.contains("no unambiguous svn tracking identity")
        {
            ErrorCategory::Ambiguity
        } else if lower.starts_with("corrupt .rev_map")
            || lower.contains("invalid svn tracking metadata")
            || lower.contains("invalid dcommit journal")
        {
            ErrorCategory::MetadataCorruption
        } else if lower.starts_with("svn_")
            || lower.starts_with("libsvn ")
            || lower.contains("svn failed with status")
            || lower.contains("git failed with status")
        {
            ErrorCategory::ExternalCommand
        } else {
            ErrorCategory::Unclassified
        };
        Self::new(category, message)
    }
}

impl From<String> for GitSvnError {
    fn from(message: String) -> Self {
        Self::unclassified(message)
    }
}

impl From<&str> for GitSvnError {
    fn from(message: &str) -> Self {
        Self::unclassified(message)
    }
}

impl From<CoordinatorError> for GitSvnError {
    fn from(error: CoordinatorError) -> Self {
        let category = match error {
            CoordinatorError::PostSubmit(_)
            | CoordinatorError::AmbiguousSubmission { .. }
            | CoordinatorError::ReconciliationFailed { .. } => ErrorCategory::PartialWrite,
            CoordinatorError::RemoteAdvanced { .. } | CoordinatorError::RemoteMismatch { .. } => {
                ErrorCategory::Ambiguity
            }
            CoordinatorError::Invalid(_) => ErrorCategory::MetadataCorruption,
            CoordinatorError::Persistence(_) | CoordinatorError::Sink(_) => {
                ErrorCategory::ExternalCommand
            }
        };
        Self::with_source(category, error)
    }
}

impl From<JournalError> for GitSvnError {
    fn from(error: JournalError) -> Self {
        let category = match error {
            JournalError::Invalid(_) | JournalError::UnsupportedVersion(_) => {
                ErrorCategory::MetadataCorruption
            }
            JournalError::Io(_) | JournalError::LockHeld(_) => ErrorCategory::ExternalCommand,
        };
        Self::with_source(category, error)
    }
}

#[cfg(test)]
mod tests {
    use super::{ErrorCategory, GitSvnError};
    use crate::dcommit::coordinator::CoordinatorError;

    #[test]
    fn coordinator_post_submit_failure_is_a_partial_write() {
        let error = GitSvnError::from(CoordinatorError::PostSubmit(
            "SVN r5 was submitted but verification failed".to_string(),
        ));

        assert_eq!(error.category(), ErrorCategory::PartialWrite);
        assert_eq!(
            error.to_string(),
            "dcommit post-submit step failed: SVN r5 was submitted but verification failed"
        );
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn command_boundary_classifies_native_auth_and_rev_map_corruption() {
        assert_eq!(
            GitSvnError::from_command_error("svn_ra_open5 failed: E170001 authentication failed")
                .category(),
            ErrorCategory::Authentication
        );
        assert_eq!(
            GitSvnError::from_command_error(
                "corrupt .rev_map /repo/.git/svn/ref/.rev_map.uuid: invalid size"
            )
            .category(),
            ErrorCategory::MetadataCorruption
        );
    }

    #[test]
    fn unsupported_text_remains_cli_compatible() {
        let error = GitSvnError::unsupported_command("branch");
        assert_eq!(error.category(), ErrorCategory::Unsupported);
        assert_eq!(error.to_string(), "unsupported in v1: branch");
    }
}
