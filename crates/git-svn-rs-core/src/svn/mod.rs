pub mod auth;
pub mod editor;
pub mod mock;
pub mod ra;
pub mod types;

#[cfg(feature = "svn-libsvn")]
pub mod libsvn;

pub use types::*;

pub trait SvnBackend {
    fn uuid(&self) -> Result<String, String>;
    fn latest_revnum(&self) -> Result<u32, String>;
    fn log(&self, start: u32, end: u32) -> Result<Vec<RevisionEvent>, String>;
}
