use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use git_svn_rs_core::dcommit::coordinator::{
    CommitSink, Coordinator, CoordinatorError, JournalPersistence, PostSubmit, PreparedDcommit,
    RemoteHead,
};
use git_svn_rs_core::dcommit::journal::{
    BatchState, DcommitJournal, DcommitTargetIdentity, EntryState, JournalEntry,
};
use git_svn_rs_core::dcommit::{
    DcommitPlan, DcommitTarget, PlannedChange, message_fingerprint, plan_fingerprint,
};

#[derive(Default)]
struct SinkState {
    remote_checks: usize,
    submitted: Vec<String>,
    plan_base_revisions: Vec<u32>,
    copy_source_revisions: Vec<Vec<u32>>,
}

struct FakeSink {
    state: Rc<RefCell<SinkState>>,
    heads: VecDeque<RemoteHead>,
    revisions: VecDeque<u64>,
}

impl CommitSink for FakeSink {
    fn remote_head(&mut self, _target: &DcommitTargetIdentity) -> Result<RemoteHead, String> {
        self.state.borrow_mut().remote_checks += 1;
        self.heads
            .pop_front()
            .ok_or_else(|| "unexpected remote-head check".to_owned())
    }

    fn submit(&mut self, plan: &DcommitPlan, _expected_base_revision: u64) -> Result<u64, String> {
        let mut state = self.state.borrow_mut();
        state.submitted.push(plan.git_commit.clone());
        state.plan_base_revisions.push(plan.base_revision);
        state.copy_source_revisions.push(
            plan.changes
                .iter()
                .filter_map(|change| change.source.as_ref().map(|source| source.revision))
                .collect(),
        );
        self.revisions
            .pop_front()
            .ok_or_else(|| "unexpected submission".to_owned())
    }
}

#[derive(Default)]
struct PostState {
    fetched: Vec<u64>,
    rebases: usize,
    fail_next_fetch: bool,
    fail_next_rebase: bool,
}

#[derive(Clone)]
struct FakePostSubmit(Rc<RefCell<PostState>>);

impl PostSubmit for FakePostSubmit {
    fn fetch_and_verify(
        &mut self,
        _target: &DcommitTargetIdentity,
        _entry: &JournalEntry,
        svn_revision: u64,
    ) -> Result<String, String> {
        let mut state = self.0.borrow_mut();
        state.fetched.push(svn_revision);
        if state.fail_next_fetch {
            state.fail_next_fetch = false;
            return Err("injected fetch failure".to_owned());
        }
        Ok(imported_oid(svn_revision))
    }

    fn rebase(&mut self, _journal: &DcommitJournal) -> Result<(), String> {
        let mut state = self.0.borrow_mut();
        state.rebases += 1;
        if state.fail_next_rebase {
            state.fail_next_rebase = false;
            return Err("injected rebase failure".to_owned());
        }
        Ok(())
    }
}

#[derive(Default)]
struct PersistenceState {
    calls: usize,
    fail_on_call: Option<usize>,
    snapshots: Vec<DcommitJournal>,
}

#[derive(Clone)]
struct FakePersistence(Rc<RefCell<PersistenceState>>);

impl JournalPersistence for FakePersistence {
    fn persist(&mut self, journal: &DcommitJournal) -> Result<(), String> {
        let mut state = self.0.borrow_mut();
        state.calls += 1;
        if state.fail_on_call == Some(state.calls) {
            return Err(format!("injected persistence failure {}", state.calls));
        }
        state.snapshots.push(journal.clone());
        Ok(())
    }
}

fn oid(character: char) -> String {
    character.to_string().repeat(40)
}

fn imported_oid(revision: u64) -> String {
    match revision {
        41 => oid('e'),
        42 => oid('f'),
        43 => oid('1'),
        _ => panic!("unexpected revision {revision}"),
    }
}

fn target_identity() -> DcommitTargetIdentity {
    DcommitTargetIdentity {
        remote_id: "svn".to_owned(),
        repository_root_url: "https://example.invalid/repos/project".to_owned(),
        repository_uuid: "12345678-1234-1234-1234-123456789abc".to_owned(),
        mapping_ref: "refs/remotes/origin/trunk".to_owned(),
        rev_map_path: ".git/svn/refs/remotes/origin/trunk/.rev_map.uuid".to_owned(),
        commit_url: "https://example.invalid/repos/project/trunk".to_owned(),
    }
}

fn prepared(count: usize, no_rebase: bool) -> PreparedDcommit {
    let commits = ['b', 'c', 'd'];
    let bases = ['a', 'b', 'c'];
    let target = target_identity();
    let plans = (0..count)
        .map(|index| DcommitPlan {
            target: DcommitTarget {
                url: target.commit_url.clone(),
                repository_root: target.repository_root_url.clone(),
                repository_uuid: target.repository_uuid.clone(),
                git_ref: target.mapping_ref.clone(),
            },
            base_revision: 40,
            git_commit: oid(commits[index]),
            message: "message".to_owned(),
            author: None,
            root_properties: Vec::new(),
            changes: vec![PlannedChange::copy_file(
                "source.txt",
                40,
                "target.txt",
                b"content",
            )],
        })
        .collect::<Vec<_>>();
    let entries = plans
        .iter()
        .enumerate()
        .map(|(index, plan)| JournalEntry {
            git_oid: plan.git_commit.clone(),
            base_oid: oid(bases[index]),
            plan_fingerprint: plan_fingerprint(plan),
            message_fingerprint: message_fingerprint(&plan.message),
            state: EntryState::Queued,
        })
        .collect::<Vec<_>>();
    PreparedDcommit {
        journal: DcommitJournal {
            target,
            original_base_revision: 40,
            original_base_oid: oid('a'),
            original_head: oid(commits[count - 1]),
            no_rebase,
            config_fingerprint: "aa".to_owned(),
            entries,
            batch_state: BatchState::Submitting,
        },
        plans,
    }
}

fn set_recovery_state(
    prepared: &mut PreparedDcommit,
    index: usize,
    revision: u32,
    state: EntryState,
) {
    prepared.plans[index].base_revision = revision;
    for change in &mut prepared.plans[index].changes {
        if let Some(source) = &mut change.source {
            source.revision = revision;
        }
    }
    prepared.journal.entries[index].plan_fingerprint = plan_fingerprint(&prepared.plans[index]);
    prepared.journal.entries[index].state = state;
}

type TestFixture = (
    Coordinator<FakeSink, FakePostSubmit, FakePersistence>,
    Rc<RefCell<SinkState>>,
    Rc<RefCell<PostState>>,
    Rc<RefCell<PersistenceState>>,
);

fn make_coordinator(
    heads: impl IntoIterator<Item = RemoteHead>,
    revisions: impl IntoIterator<Item = u64>,
) -> TestFixture {
    let sink_state = Rc::new(RefCell::new(SinkState::default()));
    let post_state = Rc::new(RefCell::new(PostState::default()));
    let persistence_state = Rc::new(RefCell::new(PersistenceState::default()));
    let coordinator = Coordinator::new(
        FakeSink {
            state: Rc::clone(&sink_state),
            heads: heads.into_iter().collect(),
            revisions: revisions.into_iter().collect(),
        },
        FakePostSubmit(Rc::clone(&post_state)),
        FakePersistence(Rc::clone(&persistence_state)),
    );
    (coordinator, sink_state, post_state, persistence_state)
}

fn head(revision: u64, tracking_oid: String) -> RemoteHead {
    RemoteHead {
        revision,
        tracking_oid,
    }
}

#[test]
fn remote_advance_fails_closed_before_submission() {
    let mut prepared = prepared(1, false);
    let (mut coordinator, sink, _, persistence) =
        make_coordinator([head(41, imported_oid(41))], []);

    assert!(matches!(
        coordinator.run(&mut prepared),
        Err(CoordinatorError::RemoteAdvanced {
            expected_revision: 40,
            actual_revision: 41
        })
    ));
    assert!(sink.borrow().submitted.is_empty());
    assert_eq!(persistence.borrow().snapshots.len(), 2);
    assert!(matches!(
        prepared.journal.entries[0].state,
        EntryState::Ready { .. }
    ));
}

#[test]
fn submitted_resume_fetches_without_duplicate_submission() {
    let mut prepared = prepared(2, false);
    set_recovery_state(
        &mut prepared,
        0,
        40,
        EntryState::Submitted { svn_revision: 41 },
    );
    let (mut coordinator, sink, post, _) = make_coordinator([head(41, imported_oid(41))], [42]);

    coordinator.run(&mut prepared).unwrap();

    assert_eq!(sink.borrow().submitted, vec![oid('c')]);
    assert_eq!(sink.borrow().plan_base_revisions, vec![41]);
    assert_eq!(sink.borrow().copy_source_revisions, vec![vec![41]]);
    assert_eq!(post.borrow().fetched, vec![41, 42]);
    assert_eq!(post.borrow().rebases, 1);
    assert_eq!(prepared.journal.batch_state, BatchState::Complete);
}

#[test]
fn second_commit_resume_advances_the_third_from_verified_tracking_state() {
    let mut prepared = prepared(3, false);
    set_recovery_state(
        &mut prepared,
        0,
        40,
        EntryState::FetchedVerified {
            svn_revision: 41,
            imported_oid: imported_oid(41),
        },
    );
    set_recovery_state(
        &mut prepared,
        1,
        41,
        EntryState::Submitted { svn_revision: 42 },
    );
    let (mut coordinator, sink, post, _) = make_coordinator([head(42, imported_oid(42))], [43]);

    coordinator.run(&mut prepared).unwrap();

    assert_eq!(sink.borrow().submitted, vec![oid('d')]);
    assert_eq!(sink.borrow().plan_base_revisions, vec![42]);
    assert_eq!(sink.borrow().copy_source_revisions, vec![vec![42]]);
    assert_eq!(post.borrow().fetched, vec![42, 43]);
    assert!(
        prepared
            .journal
            .entries
            .iter()
            .all(|entry| matches!(entry.state, EntryState::FetchedVerified { .. }))
    );
}

#[test]
fn fetch_failure_leaves_submitted_state_and_resume_does_not_resubmit() {
    let mut prepared = prepared(1, false);
    let (mut coordinator, sink, post, _) = make_coordinator([head(40, oid('a'))], [41]);
    post.borrow_mut().fail_next_fetch = true;

    assert!(matches!(
        coordinator.run(&mut prepared),
        Err(CoordinatorError::PostSubmit(message)) if message == "injected fetch failure"
    ));
    assert!(matches!(
        prepared.journal.entries[0].state,
        EntryState::Submitted { svn_revision: 41 }
    ));

    coordinator.run(&mut prepared).unwrap();
    assert_eq!(sink.borrow().submitted, vec![oid('b')]);
    assert_eq!(post.borrow().fetched, vec![41, 41]);
}

#[test]
fn no_rebase_finishes_with_a_durable_complete_tombstone() {
    let mut prepared = prepared(1, true);
    let (mut coordinator, _, post, persistence) = make_coordinator([head(40, oid('a'))], [41]);

    coordinator.run(&mut prepared).unwrap();

    assert_eq!(prepared.journal.batch_state, BatchState::Complete);
    assert_eq!(post.borrow().rebases, 0);
    assert_eq!(
        persistence.borrow().snapshots.last().unwrap().batch_state,
        BatchState::Complete
    );
}

#[test]
fn rebase_pending_failure_is_retryable_without_any_submission() {
    let mut prepared = prepared(1, false);
    set_recovery_state(
        &mut prepared,
        0,
        40,
        EntryState::FetchedVerified {
            svn_revision: 41,
            imported_oid: imported_oid(41),
        },
    );
    prepared.journal.batch_state = BatchState::RebasePending;
    let (mut coordinator, sink, post, _) = make_coordinator([], []);
    post.borrow_mut().fail_next_rebase = true;

    assert!(matches!(
        coordinator.run(&mut prepared),
        Err(CoordinatorError::PostSubmit(message)) if message == "injected rebase failure"
    ));
    assert_eq!(prepared.journal.batch_state, BatchState::RebasePending);
    assert!(sink.borrow().submitted.is_empty());

    coordinator.run(&mut prepared).unwrap();
    assert_eq!(prepared.journal.batch_state, BatchState::Complete);
    assert_eq!(post.borrow().rebases, 2);
    assert!(sink.borrow().submitted.is_empty());
}

#[test]
fn fingerprint_tampering_is_rejected_before_persistence_or_remote_access() {
    for tamper in [
        |prepared: &mut PreparedDcommit| prepared.plans[0].message.push('!'),
        |prepared: &mut PreparedDcommit| {
            prepared.plans[0].changes[0].content = Some(b"tampered".to_vec())
        },
    ] {
        let mut prepared = prepared(1, false);
        tamper(&mut prepared);
        let (mut coordinator, sink, _, persistence) = make_coordinator([], []);

        assert!(matches!(
            coordinator.run(&mut prepared),
            Err(CoordinatorError::Invalid(_))
        ));
        assert_eq!(persistence.borrow().calls, 0);
        assert_eq!(sink.borrow().remote_checks, 0);
        assert!(sink.borrow().submitted.is_empty());
    }
}

#[test]
fn persistence_faults_stop_before_or_mark_remote_side_effects() {
    let mut before_ready = prepared(1, false);
    let (mut coordinator, sink, _, persistence) = make_coordinator([head(40, oid('a'))], [41]);
    persistence.borrow_mut().fail_on_call = Some(1);
    assert!(matches!(
        coordinator.run(&mut before_ready),
        Err(CoordinatorError::Persistence(_))
    ));
    assert_eq!(sink.borrow().remote_checks, 0);

    let mut before_submit = prepared(1, false);
    let (mut coordinator, sink, _, persistence) = make_coordinator([head(40, oid('a'))], [41]);
    persistence.borrow_mut().fail_on_call = Some(2);
    assert!(matches!(
        coordinator.run(&mut before_submit),
        Err(CoordinatorError::Persistence(_))
    ));
    assert!(sink.borrow().submitted.is_empty());

    let mut before_submit_call = prepared(1, false);
    let (mut coordinator, sink, _, persistence) = make_coordinator([head(40, oid('a'))], [41]);
    persistence.borrow_mut().fail_on_call = Some(3);
    assert!(matches!(
        coordinator.run(&mut before_submit_call),
        Err(CoordinatorError::Persistence(_))
    ));
    assert!(sink.borrow().submitted.is_empty());

    let mut after_submit = prepared(1, false);
    let (mut coordinator, sink, _, persistence) = make_coordinator([head(40, oid('a'))], [41]);
    persistence.borrow_mut().fail_on_call = Some(4);
    assert!(matches!(
        coordinator.run(&mut after_submit),
        Err(CoordinatorError::AmbiguousSubmission {
            svn_revision: Some(41),
            ..
        })
    ));
    assert_eq!(sink.borrow().submitted, vec![oid('b')]);
    assert!(matches!(
        after_submit.journal.entries[0].state,
        EntryState::Submitted { svn_revision: 41 }
    ));
}

#[test]
fn submit_error_leaves_durable_in_flight_state_and_retry_does_not_resubmit() {
    let mut prepared = prepared(1, false);
    let (mut coordinator, sink, _, persistence) = make_coordinator([head(40, oid('a'))], []);

    assert!(matches!(
        coordinator.run(&mut prepared),
        Err(CoordinatorError::AmbiguousSubmission {
            svn_revision: None,
            ..
        })
    ));
    assert_eq!(sink.borrow().submitted, vec![oid('b')]);
    assert!(matches!(
        prepared.journal.entries[0].state,
        EntryState::SubmissionInFlight {
            expected_base_revision: 40,
            ..
        }
    ));
    assert!(matches!(
        persistence.borrow().snapshots.last().unwrap().entries[0].state,
        EntryState::SubmissionInFlight { .. }
    ));

    assert!(matches!(
        coordinator.run(&mut prepared),
        Err(CoordinatorError::AmbiguousSubmission {
            svn_revision: None,
            ..
        })
    ));
    assert_eq!(sink.borrow().submitted, vec![oid('b')]);
}

#[test]
fn manual_adoption_verifies_before_persisting_and_never_resubmits() {
    let mut prepared = prepared(1, true);
    set_recovery_state(
        &mut prepared,
        0,
        40,
        EntryState::SubmissionInFlight {
            expected_base_revision: 40,
            expected_tracking_oid: oid('a'),
        },
    );
    let (mut coordinator, sink, post, persistence) = make_coordinator([], []);

    coordinator.adopt_in_flight(&mut prepared, 41).unwrap();

    assert!(sink.borrow().submitted.is_empty());
    assert_eq!(post.borrow().fetched, vec![41]);
    assert!(matches!(
        prepared.journal.entries[0].state,
        EntryState::FetchedVerified {
            svn_revision: 41,
            ..
        }
    ));
    assert!(matches!(
        persistence.borrow().snapshots.last().unwrap().entries[0].state,
        EntryState::FetchedVerified {
            svn_revision: 41,
            ..
        }
    ));

    coordinator.run(&mut prepared).unwrap();
    assert!(sink.borrow().submitted.is_empty());
    assert_eq!(prepared.journal.batch_state, BatchState::Complete);
}

#[test]
fn failed_manual_adoption_keeps_the_durable_in_flight_state() {
    let mut prepared = prepared(1, true);
    set_recovery_state(
        &mut prepared,
        0,
        40,
        EntryState::SubmissionInFlight {
            expected_base_revision: 40,
            expected_tracking_oid: oid('a'),
        },
    );
    let (mut coordinator, sink, post, persistence) = make_coordinator([], []);
    post.borrow_mut().fail_next_fetch = true;

    assert!(matches!(
        coordinator.adopt_in_flight(&mut prepared, 41),
        Err(CoordinatorError::ReconciliationFailed {
            svn_revision: 41,
            ..
        })
    ));
    assert!(sink.borrow().submitted.is_empty());
    assert_eq!(post.borrow().fetched, vec![41]);
    assert!(persistence.borrow().snapshots.is_empty());
    assert!(matches!(
        prepared.journal.entries[0].state,
        EntryState::SubmissionInFlight { .. }
    ));
}
