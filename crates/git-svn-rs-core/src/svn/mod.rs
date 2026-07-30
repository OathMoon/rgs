pub mod auth;
pub mod cli;
pub mod editor;
pub mod mock;
pub mod ra;
pub mod types;

#[cfg(feature = "svn-libsvn")]
pub mod libsvn;

pub use types::*;

pub(crate) fn target_without_peg_revision(target: &str) -> String {
    if target.ends_with('@') || !peg_sensitive(target) {
        target.to_string()
    } else {
        format!("{target}@")
    }
}

pub(crate) fn target_at_revision(target: &str, revision: u32) -> String {
    format!("{target}@{revision}")
}

fn peg_sensitive(target: &str) -> bool {
    target.contains('@')
        || target
            .as_bytes()
            .windows(3)
            .any(|window| window.eq_ignore_ascii_case(b"%40"))
}

pub trait SvnBackend {
    fn uuid(&self) -> Result<String, String>;
    fn latest_revnum(&self) -> Result<u32, String>;
    fn log(&self, start: u32, end: u32) -> Result<Vec<RevisionEvent>, String>;
}

#[cfg(test)]
mod tests {
    use super::{target_at_revision, target_without_peg_revision};

    #[test]
    fn peg_sensitive_targets_get_an_explicit_empty_peg_revision() {
        assert_eq!(
            target_without_peg_revision("file:///repo/trunk@main"),
            "file:///repo/trunk@main@"
        );
        assert_eq!(
            target_without_peg_revision("file:///repo/trunk%40main"),
            "file:///repo/trunk%40main@"
        );
        assert_eq!(
            target_without_peg_revision("file:///repo/trunk%40main@"),
            "file:///repo/trunk%40main@"
        );
        assert_eq!(
            target_without_peg_revision("file:///repo/trunk"),
            "file:///repo/trunk"
        );
        assert_eq!(
            target_at_revision("file:///repo/trunk%40main", 7),
            "file:///repo/trunk%40main@7"
        );
    }
}
