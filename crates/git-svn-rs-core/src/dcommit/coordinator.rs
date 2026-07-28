use std::fmt;

use super::diff_planner::DcommitPlan;
use super::fingerprint::{message_fingerprint, plan_fingerprint};
use super::journal::{BatchState, DcommitJournal, DcommitTargetIdentity, EntryState, JournalEntry};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteHead {
    pub revision: u64,
    pub tracking_oid: String,
}

pub trait CommitSink {
    fn remote_head(&mut self, target: &DcommitTargetIdentity) -> Result<RemoteHead, String>;

    fn submit(&mut self, plan: &DcommitPlan, expected_base_revision: u64) -> Result<u64, String>;
}

pub trait PostSubmit {
    fn fetch_and_verify(
        &mut self,
        target: &DcommitTargetIdentity,
        entry: &JournalEntry,
        svn_revision: u64,
    ) -> Result<String, String>;

    /// Completes the local rebase. Implementations must be retry-safe because
    /// `RebasePending` is persisted before this method is called.
    fn rebase(&mut self, journal: &DcommitJournal) -> Result<(), String>;
}

pub trait JournalPersistence {
    fn persist(&mut self, journal: &DcommitJournal) -> Result<(), String>;
}

#[derive(Clone, Debug)]
pub struct PreparedDcommit {
    pub journal: DcommitJournal,
    pub plans: Vec<DcommitPlan>,
}

impl PreparedDcommit {
    pub fn validate(&self) -> Result<(), CoordinatorError> {
        self.journal
            .validate()
            .map_err(|error| CoordinatorError::Invalid(error.to_string()))?;
        if self.plans.len() != self.journal.entries.len() {
            return Err(CoordinatorError::Invalid(format!(
                "prepared plan count {} does not match journal entry count {}",
                self.plans.len(),
                self.journal.entries.len()
            )));
        }
        for (index, (plan, entry)) in self
            .plans
            .iter()
            .zip(self.journal.entries.iter())
            .enumerate()
        {
            if plan.git_commit != entry.git_oid {
                return Err(CoordinatorError::Invalid(format!(
                    "plan {index} is for Git commit {}, expected {}",
                    plan.git_commit, entry.git_oid
                )));
            }
            let actual_message_fingerprint = message_fingerprint(&plan.message);
            if actual_message_fingerprint != entry.message_fingerprint {
                return Err(CoordinatorError::Invalid(format!(
                    "plan {index} message fingerprint does not match the journal entry"
                )));
            }
            let actual_plan_fingerprint = plan_fingerprint(plan);
            if actual_plan_fingerprint != entry.plan_fingerprint {
                return Err(CoordinatorError::Invalid(format!(
                    "plan {index} fingerprint does not match the journal entry"
                )));
            }
            if plan.target.url != self.journal.target.commit_url
                || plan.target.repository_root != self.journal.target.repository_root_url
                || plan.target.repository_uuid != self.journal.target.repository_uuid
                || plan.target.git_ref != self.journal.target.mapping_ref
            {
                return Err(CoordinatorError::Invalid(format!(
                    "plan {index} target does not match the journal target"
                )));
            }
        }
        Ok(())
    }
}

pub struct Coordinator<S, P, J> {
    sink: S,
    post_submit: P,
    persistence: J,
}

impl<S, P, J> Coordinator<S, P, J>
where
    S: CommitSink,
    P: PostSubmit,
    J: JournalPersistence,
{
    pub fn new(sink: S, post_submit: P, persistence: J) -> Self {
        Self {
            sink,
            post_submit,
            persistence,
        }
    }

    /// Reconciles an ambiguous submission using a revision selected by the
    /// operator. The durable journal advances only after the normal
    /// post-submit import and plan verification succeeds.
    pub fn adopt_in_flight(
        &mut self,
        prepared: &mut PreparedDcommit,
        svn_revision: u64,
    ) -> Result<(), CoordinatorError> {
        prepared.validate()?;
        let index = prepared
            .journal
            .entries
            .iter()
            .position(|entry| matches!(entry.state, EntryState::SubmissionInFlight { .. }))
            .ok_or_else(|| {
                CoordinatorError::Invalid(
                    "manual revision adoption requires a SubmissionInFlight journal entry"
                        .to_owned(),
                )
            })?;
        let expected_base_revision = match prepared.journal.entries[index].state {
            EntryState::SubmissionInFlight {
                expected_base_revision,
                ..
            } => expected_base_revision,
            _ => unreachable!("selected an in-flight entry"),
        };
        if svn_revision <= expected_base_revision {
            return Err(CoordinatorError::Invalid(format!(
                "adopted SVN revision r{svn_revision} must be newer than the submission base r{expected_base_revision}"
            )));
        }

        let imported_oid = self
            .post_submit
            .fetch_and_verify(
                &prepared.journal.target,
                &prepared.journal.entries[index],
                svn_revision,
            )
            .map_err(|detail| CoordinatorError::ReconciliationFailed {
                svn_revision,
                detail,
            })?;
        prepared.journal.entries[index].state = EntryState::FetchedVerified {
            svn_revision,
            imported_oid,
        };
        self.persist(&prepared.journal)
    }

    pub fn run(&mut self, prepared: &mut PreparedDcommit) -> Result<(), CoordinatorError> {
        prepared.validate()?;

        // This first durable write establishes the entire oldest-first queue before
        // any remote observation or side effect.
        self.persist(&prepared.journal)?;

        loop {
            match prepared.journal.batch_state {
                BatchState::Complete => return Ok(()),
                BatchState::RebasePending => {
                    self.post_submit
                        .rebase(&prepared.journal)
                        .map_err(CoordinatorError::PostSubmit)?;
                    prepared.journal.batch_state = BatchState::Complete;
                    self.persist(&prepared.journal)?;
                    return Ok(());
                }
                BatchState::Submitting => {}
            }

            let Some(index) = prepared
                .journal
                .entries
                .iter()
                .position(|entry| !matches!(entry.state, EntryState::FetchedVerified { .. }))
            else {
                prepared.journal.batch_state = if prepared.journal.no_rebase {
                    BatchState::Complete
                } else {
                    BatchState::RebasePending
                };
                self.persist(&prepared.journal)?;
                continue;
            };

            match prepared.journal.entries[index].state.clone() {
                EntryState::Queued => {
                    let (revision, tracking_oid) = expected_base(&prepared.journal, index)?;
                    set_plan_base_revision(&mut prepared.plans[index], revision)?;
                    prepared.journal.entries[index].plan_fingerprint =
                        plan_fingerprint(&prepared.plans[index]);
                    prepared.journal.entries[index].state = EntryState::Ready {
                        expected_base_revision: revision,
                        expected_tracking_oid: tracking_oid,
                    };
                    self.persist(&prepared.journal)?;
                }
                EntryState::Ready {
                    expected_base_revision,
                    expected_tracking_oid,
                } => {
                    let actual = self
                        .sink
                        .remote_head(&prepared.journal.target)
                        .map_err(CoordinatorError::Sink)?;
                    if actual.revision > expected_base_revision {
                        return Err(CoordinatorError::RemoteAdvanced {
                            expected_revision: expected_base_revision,
                            actual_revision: actual.revision,
                        });
                    }
                    if actual.revision != expected_base_revision
                        || actual.tracking_oid != expected_tracking_oid
                    {
                        return Err(CoordinatorError::RemoteMismatch {
                            expected_revision: expected_base_revision,
                            expected_tracking_oid,
                            actual,
                        });
                    }

                    prepared.journal.entries[index].state = EntryState::SubmissionInFlight {
                        expected_base_revision,
                        expected_tracking_oid,
                    };
                    self.persist(&prepared.journal)?;
                    let svn_revision = match self
                        .sink
                        .submit(&prepared.plans[index], expected_base_revision)
                    {
                        Ok(revision) => revision,
                        Err(error) => {
                            return Err(CoordinatorError::AmbiguousSubmission {
                                svn_revision: None,
                                detail: format!(
                                    "the submit command failed after the in-flight marker was persisted: {error}"
                                ),
                            });
                        }
                    };
                    if svn_revision <= expected_base_revision {
                        return Err(CoordinatorError::Invalid(format!(
                            "commit sink returned SVN revision {svn_revision} after base revision {expected_base_revision}"
                        )));
                    }
                    prepared.journal.entries[index].state = EntryState::Submitted { svn_revision };
                    if let Err(error) = self.persist(&prepared.journal) {
                        return Err(CoordinatorError::AmbiguousSubmission {
                            svn_revision: Some(svn_revision),
                            detail: format!("the submitted state could not be persisted: {error}"),
                        });
                    }
                }
                EntryState::SubmissionInFlight {
                    expected_base_revision,
                    ..
                } => {
                    return Err(CoordinatorError::AmbiguousSubmission {
                        svn_revision: None,
                        detail: format!(
                            "the durable journal records an interrupted submission after base r{expected_base_revision}"
                        ),
                    });
                }
                EntryState::Submitted { svn_revision } => {
                    let imported_oid = self
                        .post_submit
                        .fetch_and_verify(
                            &prepared.journal.target,
                            &prepared.journal.entries[index],
                            svn_revision,
                        )
                        .map_err(CoordinatorError::PostSubmit)?;
                    prepared.journal.entries[index].state = EntryState::FetchedVerified {
                        svn_revision,
                        imported_oid,
                    };
                    self.persist(&prepared.journal)?;
                }
                EntryState::FetchedVerified { .. } => unreachable!("selected an active entry"),
            }
        }
    }

    fn persist(&mut self, journal: &DcommitJournal) -> Result<(), CoordinatorError> {
        journal
            .validate()
            .map_err(|error| CoordinatorError::Invalid(error.to_string()))?;
        self.persistence
            .persist(journal)
            .map_err(CoordinatorError::Persistence)
    }
}

fn set_plan_base_revision(plan: &mut DcommitPlan, revision: u64) -> Result<(), CoordinatorError> {
    let revision = u32::try_from(revision).map_err(|_| {
        CoordinatorError::Invalid(format!(
            "SVN base revision {revision} cannot be represented by a dcommit plan"
        ))
    })?;
    plan.base_revision = revision;
    for change in &mut plan.changes {
        if let Some(source) = &mut change.source {
            source.revision = revision;
        }
    }
    Ok(())
}

fn expected_base(
    journal: &DcommitJournal,
    index: usize,
) -> Result<(u64, String), CoordinatorError> {
    if index == 0 {
        return Ok((
            journal.original_base_revision,
            journal.original_base_oid.clone(),
        ));
    }
    match &journal.entries[index - 1].state {
        EntryState::FetchedVerified {
            svn_revision,
            imported_oid,
        } => Ok((*svn_revision, imported_oid.clone())),
        _ => Err(CoordinatorError::Invalid(format!(
            "queued entry {index} does not follow a fetched and verified entry"
        ))),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoordinatorError {
    Invalid(String),
    Persistence(String),
    Sink(String),
    PostSubmit(String),
    RemoteAdvanced {
        expected_revision: u64,
        actual_revision: u64,
    },
    RemoteMismatch {
        expected_revision: u64,
        expected_tracking_oid: String,
        actual: RemoteHead,
    },
    AmbiguousSubmission {
        svn_revision: Option<u64>,
        detail: String,
    },
    ReconciliationFailed {
        svn_revision: u64,
        detail: String,
    },
}

impl fmt::Display for CoordinatorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(formatter, "invalid prepared dcommit: {message}"),
            Self::Persistence(message) => {
                write!(formatter, "could not persist dcommit journal: {message}")
            }
            Self::Sink(message) => write!(formatter, "dcommit sink failed: {message}"),
            Self::PostSubmit(message) => {
                write!(formatter, "dcommit post-submit step failed: {message}")
            }
            Self::RemoteAdvanced {
                expected_revision,
                actual_revision,
            } => write!(
                formatter,
                "SVN remote advanced from expected r{expected_revision} to r{actual_revision}; refusing to submit"
            ),
            Self::RemoteMismatch {
                expected_revision,
                expected_tracking_oid,
                actual,
            } => write!(
                formatter,
                "SVN remote head mismatch: expected r{expected_revision}/{expected_tracking_oid}, found r{}/{}",
                actual.revision, actual.tracking_oid
            ),
            Self::AmbiguousSubmission {
                svn_revision,
                detail,
            } => match svn_revision {
                Some(revision) => write!(
                    formatter,
                    "SVN r{revision} may have been submitted but its durable outcome is ambiguous ({detail}); inspect the target SVN log and use --adopt-revision REV only after identifying the matching revision; refusing automatic retry"
                ),
                None => write!(
                    formatter,
                    "SVN submission outcome is ambiguous ({detail}); inspect the target SVN log and use --adopt-revision REV only after identifying the matching revision; refusing automatic retry"
                ),
            },
            Self::ReconciliationFailed {
                svn_revision,
                detail,
            } => write!(
                formatter,
                "could not adopt SVN r{svn_revision}: post-submit import and plan verification failed ({detail}); the journal remains in-flight"
            ),
        }
    }
}

impl std::error::Error for CoordinatorError {}
