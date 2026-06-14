use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitSvnError {
    #[error("unsupported in v1: {0}")]
    UnsupportedCommand(String),
}
