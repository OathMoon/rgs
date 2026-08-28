//! Core parsing, repository, SVN transport, import, and command orchestration for
//! `git-svn-rs`.
//!
//! The supported v0.1 API is intentionally facade-oriented. Internal recovery,
//! planning, FFI, and publication modules may change without preserving their
//! module paths; see `.plans/public-api-allowlist.md` in the source repository.

pub mod authors;
pub mod cli;
pub mod commands;
pub mod config;
pub mod dcommit;
pub mod diagnostics;
pub mod error;
pub mod fast_import;
pub mod fetch_editor;
mod filesystem;
pub mod filters;
pub mod git;
pub mod git_svn_id;
pub mod glob_spec;
pub mod import;
pub mod import_transaction;
pub mod log_formatter;
pub mod mapping;
pub mod metadata;
pub mod migration;
pub mod path_url;
pub mod rev_map;
pub mod svn;
mod tracking_state;
