//! Typed dcommit plans, durable coordination, and the stable v0.1 facade.
//!
//! Planning helpers are re-exported here so implementation modules can remain
//! private. Durable journal/coordinator modules stay public because recovery
//! tooling and compatibility tests consume their typed state directly.

mod attributes;
mod commit_editor;
pub mod coordinator;
mod diff_planner;
mod fingerprint;
pub mod journal;
pub(crate) mod journal_persistence;
pub mod journal_registry;
mod plan_builder;
mod prepared_builder;
mod property_mapper;
pub(crate) mod tree_projection;

pub use attributes::{merge_attribute_properties, svn_file_properties};
pub use commit_editor::{PathEnsurer, SvnCommitEditor};
pub(crate) use diff_planner::normalize_commit_path;
pub use diff_planner::{
    ChangeMetadata, CopySource, DcommitPlan, DcommitTarget, GitDiffChange, GitDiffPlanner,
    PlannedChange, PlannedChangeKind, PlannedCommit, PropertyChange,
};
pub use fingerprint::{
    RecoveryFetchIntent, RecoveryFingerprintInput, canonical_message_bytes, canonical_plan_bytes,
    canonical_recovery_config_bytes, message_fingerprint, plan_fingerprint,
    recovery_config_fingerprint,
};
pub(crate) use fingerprint::{
    legacy_recovery_config_fingerprint_v2, legacy_recovery_config_fingerprint_v3,
};
pub use plan_builder::{DcommitPlanBuilder, DcommitPlanRequest};
pub use prepared_builder::{
    PreparedDcommitBuildError, PreparedDcommitRequest, build_prepared_dcommit,
};
pub use property_mapper::PropertyMapper;
