use std::collections::HashSet;
use std::fmt;

use super::coordinator::PreparedDcommit;
use super::diff_planner::DcommitPlan;
use super::fingerprint::{message_fingerprint, plan_fingerprint};
use super::journal::{BatchState, DcommitJournal, DcommitTargetIdentity, EntryState, JournalEntry};

#[derive(Clone, Debug)]
pub struct PreparedDcommitRequest {
    pub target: DcommitTargetIdentity,
    pub original_base_revision: u64,
    pub original_base_oid: String,
    pub original_head: String,
    pub no_rebase: bool,
    pub config_fingerprint: String,
    /// Plans must be ordered from the oldest Git commit to the newest.
    pub plans: Vec<DcommitPlan>,
}

pub fn build_prepared_dcommit(
    request: PreparedDcommitRequest,
) -> Result<PreparedDcommit, PreparedDcommitBuildError> {
    if request.plans.is_empty() {
        return Err(PreparedDcommitBuildError::Invalid(
            "dcommit plan queue is empty".to_owned(),
        ));
    }

    let expected_target = &request.target;
    let mut seen_oids = HashSet::with_capacity(request.plans.len());
    for (index, plan) in request.plans.iter().enumerate() {
        if plan.target.url != expected_target.commit_url
            || plan.target.repository_root != expected_target.repository_root_url
            || plan.target.repository_uuid != expected_target.repository_uuid
            || plan.target.git_ref != expected_target.mapping_ref
        {
            return Err(PreparedDcommitBuildError::Invalid(format!(
                "plan {index} target does not match the dcommit target"
            )));
        }
        if !seen_oids.insert(plan.git_commit.as_str()) {
            return Err(PreparedDcommitBuildError::Invalid(format!(
                "plan {index} repeats Git commit {} in the oldest-first chain",
                plan.git_commit
            )));
        }
    }

    if request.plans.last().map(|plan| plan.git_commit.as_str())
        != Some(request.original_head.as_str())
    {
        return Err(PreparedDcommitBuildError::Invalid(
            "newest plan Git commit does not match the original HEAD".to_owned(),
        ));
    }

    let mut base_oid = request.original_base_oid.clone();
    let entries = request
        .plans
        .iter()
        .map(|plan| {
            let entry = JournalEntry {
                git_oid: plan.git_commit.clone(),
                base_oid: base_oid.clone(),
                plan_fingerprint: plan_fingerprint(plan),
                message_fingerprint: message_fingerprint(&plan.message),
                state: EntryState::Queued,
            };
            base_oid.clone_from(&plan.git_commit);
            entry
        })
        .collect();

    let prepared = PreparedDcommit {
        journal: DcommitJournal {
            target: request.target,
            original_base_revision: request.original_base_revision,
            original_base_oid: request.original_base_oid,
            original_head: request.original_head,
            no_rebase: request.no_rebase,
            config_fingerprint: request.config_fingerprint,
            entries,
            batch_state: BatchState::Submitting,
        },
        plans: request.plans,
    };
    prepared
        .validate()
        .map_err(|error| PreparedDcommitBuildError::Invalid(error.to_string()))?;
    Ok(prepared)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreparedDcommitBuildError {
    Invalid(String),
}

impl fmt::Display for PreparedDcommitBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(formatter, "invalid prepared dcommit: {message}"),
        }
    }
}

impl std::error::Error for PreparedDcommitBuildError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcommit::diff_planner::DcommitTarget;

    fn oid(value: char) -> String {
        value.to_string().repeat(40)
    }

    fn target() -> DcommitTargetIdentity {
        DcommitTargetIdentity {
            remote_id: "svn".to_owned(),
            repository_root_url: "file:///repo".to_owned(),
            repository_uuid: "uuid".to_owned(),
            mapping_ref: "refs/remotes/git-svn".to_owned(),
            rev_map_path: ".git/svn/git-svn/.rev_map.uuid".to_owned(),
            commit_url: "file:///repo/trunk".to_owned(),
        }
    }

    fn plan(git_commit: String, message: &str) -> DcommitPlan {
        DcommitPlan {
            target: DcommitTarget {
                url: "file:///repo/trunk".to_owned(),
                repository_root: "file:///repo".to_owned(),
                repository_uuid: "uuid".to_owned(),
                git_ref: "refs/remotes/git-svn".to_owned(),
            },
            base_revision: 7,
            git_commit,
            message: message.to_owned(),
            author: None,
            root_properties: Vec::new(),
            changes: Vec::new(),
        }
    }

    fn request(plans: Vec<DcommitPlan>) -> PreparedDcommitRequest {
        let original_head = plans
            .last()
            .map_or_else(|| oid('c'), |plan| plan.git_commit.clone());
        PreparedDcommitRequest {
            target: target(),
            original_base_revision: 7,
            original_base_oid: oid('a'),
            original_head,
            no_rebase: false,
            config_fingerprint: "12".repeat(32),
            plans,
        }
    }

    #[test]
    fn builds_oldest_first_multi_commit_queue() {
        let plans = vec![plan(oid('b'), "first"), plan(oid('c'), "second")];
        let prepared = build_prepared_dcommit(request(plans.clone())).unwrap();

        assert_eq!(prepared.journal.batch_state, BatchState::Submitting);
        assert_eq!(prepared.journal.entries[0].base_oid, oid('a'));
        assert_eq!(prepared.journal.entries[0].git_oid, oid('b'));
        assert_eq!(prepared.journal.entries[1].base_oid, oid('b'));
        assert_eq!(prepared.journal.entries[1].git_oid, oid('c'));
        assert!(
            prepared
                .journal
                .entries
                .iter()
                .all(|entry| entry.state == EntryState::Queued)
        );
        assert_eq!(prepared.plans, plans);
    }

    #[test]
    fn rejects_target_and_head_mismatches() {
        let mut wrong_target = request(vec![plan(oid('b'), "message")]);
        wrong_target.plans[0].target.url = "file:///other/trunk".to_owned();
        assert!(build_prepared_dcommit(wrong_target).is_err());

        let mut wrong_head = request(vec![plan(oid('b'), "message")]);
        wrong_head.original_head = oid('c');
        assert!(build_prepared_dcommit(wrong_head).is_err());
    }

    #[test]
    fn records_production_plan_and_message_fingerprints() {
        let plans = vec![plan(oid('b'), "subject\n\nbody\n")];
        let expected_plan = plan_fingerprint(&plans[0]);
        let expected_message = message_fingerprint(&plans[0].message);
        let prepared = build_prepared_dcommit(request(plans)).unwrap();

        assert_eq!(prepared.journal.entries[0].plan_fingerprint, expected_plan);
        assert_eq!(
            prepared.journal.entries[0].message_fingerprint,
            expected_message
        );
    }

    #[test]
    fn rejects_empty_and_repeated_commit_chains() {
        assert!(build_prepared_dcommit(request(Vec::new())).is_err());

        let repeated = oid('b');
        let plans = vec![plan(repeated.clone(), "first"), plan(repeated, "second")];
        assert!(build_prepared_dcommit(request(plans)).is_err());
    }
}
