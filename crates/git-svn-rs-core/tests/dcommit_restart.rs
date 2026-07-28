use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use git_svn_rs_core::dcommit::coordinator::{
    CommitSink, Coordinator, CoordinatorError, JournalPersistence, PostSubmit, PreparedDcommit,
    RemoteHead,
};
use git_svn_rs_core::dcommit::journal::{
    BatchState, DcommitJournal, DcommitTargetIdentity, EntryState, JournalEntry, JournalLock,
    JournalStore,
};
use git_svn_rs_core::dcommit::{DcommitPlan, DcommitTarget, message_fingerprint, plan_fingerprint};

#[derive(Default)]
struct SinkState {
    remote_checks: usize,
    submissions: usize,
}

struct RecordingSink {
    state: Rc<RefCell<SinkState>>,
    heads: VecDeque<RemoteHead>,
    revisions: VecDeque<u64>,
}

impl CommitSink for RecordingSink {
    fn remote_head(&mut self, _target: &DcommitTargetIdentity) -> Result<RemoteHead, String> {
        self.state.borrow_mut().remote_checks += 1;
        self.heads
            .pop_front()
            .ok_or_else(|| "unexpected remote-head check".to_owned())
    }

    fn submit(&mut self, _plan: &DcommitPlan, _expected_base_revision: u64) -> Result<u64, String> {
        self.state.borrow_mut().submissions += 1;
        self.revisions
            .pop_front()
            .ok_or_else(|| "unexpected submission".to_owned())
    }
}

#[derive(Default)]
struct PostState {
    fetches: Vec<u64>,
    rebases: usize,
    fail_next_rebase: bool,
}

struct RecordingPostSubmit(Rc<RefCell<PostState>>);

impl PostSubmit for RecordingPostSubmit {
    fn fetch_and_verify(
        &mut self,
        _target: &DcommitTargetIdentity,
        _entry: &JournalEntry,
        svn_revision: u64,
    ) -> Result<String, String> {
        self.0.borrow_mut().fetches.push(svn_revision);
        Ok(imported_oid())
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

struct StorePersistence {
    store: JournalStore,
    _lock: JournalLock,
    calls: usize,
    fail_on_call: Option<usize>,
}

impl StorePersistence {
    fn acquire(store: JournalStore, fail_on_call: Option<usize>) -> Self {
        let lock = store.acquire_lock().unwrap();
        Self {
            store,
            _lock: lock,
            calls: 0,
            fail_on_call,
        }
    }
}

impl JournalPersistence for StorePersistence {
    fn persist(&mut self, journal: &DcommitJournal) -> Result<(), String> {
        self.calls += 1;
        if self.fail_on_call == Some(self.calls) {
            return Err(format!("injected save failure {}", self.calls));
        }
        self.store
            .save(&self._lock, journal)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

struct SubmittedAckLossPersistence {
    store: JournalStore,
    _lock: JournalLock,
    failed: bool,
}

impl SubmittedAckLossPersistence {
    fn acquire(store: JournalStore) -> Self {
        let lock = store.acquire_lock().unwrap();
        Self {
            store,
            _lock: lock,
            failed: false,
        }
    }
}

impl JournalPersistence for SubmittedAckLossPersistence {
    fn persist(&mut self, journal: &DcommitJournal) -> Result<(), String> {
        self.store
            .save(&self._lock, journal)
            .map_err(|error| error.to_string())?;
        if !self.failed
            && journal
                .entries
                .iter()
                .any(|entry| matches!(entry.state, EntryState::Submitted { .. }))
        {
            self.failed = true;
            return Err("injected submitted-save acknowledgement loss".to_owned());
        }
        Ok(())
    }
}

fn oid(character: char) -> String {
    character.to_string().repeat(40)
}

fn imported_oid() -> String {
    oid('c')
}

fn target_identity() -> DcommitTargetIdentity {
    DcommitTargetIdentity {
        remote_id: "svn".to_owned(),
        repository_root_url: "file:///repository".to_owned(),
        repository_uuid: "12345678-1234-1234-1234-123456789abc".to_owned(),
        mapping_ref: "refs/remotes/origin/trunk".to_owned(),
        rev_map_path: ".git/svn/refs/remotes/origin/trunk/.rev_map.uuid".to_owned(),
        commit_url: "file:///repository/trunk".to_owned(),
    }
}

fn journal(entry_state: EntryState, batch_state: BatchState, no_rebase: bool) -> DcommitJournal {
    // Every fixture has one Git commit based on SVN r40. Submitted and
    // FetchedVerified record the resulting r41, but the durable plan remains
    // the plan that was executed against r40.
    let plan = plan(40);
    DcommitJournal {
        target: target_identity(),
        original_base_revision: 40,
        original_base_oid: oid('a'),
        original_head: oid('b'),
        no_rebase,
        config_fingerprint: "1010".to_owned(),
        entries: vec![JournalEntry {
            git_oid: oid('b'),
            base_oid: oid('a'),
            plan_fingerprint: plan_fingerprint(&plan),
            message_fingerprint: message_fingerprint(&plan.message),
            state: entry_state,
        }],
        batch_state,
    }
}

fn prepared(journal: DcommitJournal) -> PreparedDcommit {
    PreparedDcommit {
        plans: vec![plan(40)],
        journal,
    }
}

fn plan(base_revision: u32) -> DcommitPlan {
    let target = target_identity();
    DcommitPlan {
        target: DcommitTarget {
            url: target.commit_url,
            repository_root: target.repository_root_url,
            repository_uuid: target.repository_uuid,
            git_ref: target.mapping_ref,
        },
        base_revision,
        git_commit: oid('b'),
        message: "restart test".to_owned(),
        author: None,
        root_properties: Vec::new(),
        changes: Vec::new(),
    }
}

fn save_initial(store: &JournalStore, journal: &DcommitJournal) {
    let lock = store.acquire_lock().unwrap();
    store.save(&lock, journal).unwrap();
}

fn load(store: &JournalStore) -> DcommitJournal {
    store.load().unwrap().expect("journal snapshot")
}

fn sink(
    state: &Rc<RefCell<SinkState>>,
    heads: impl IntoIterator<Item = RemoteHead>,
    revisions: impl IntoIterator<Item = u64>,
) -> RecordingSink {
    RecordingSink {
        state: Rc::clone(state),
        heads: heads.into_iter().collect(),
        revisions: revisions.into_iter().collect(),
    }
}

#[test]
fn restart_from_submitted_snapshot_fetches_without_resubmitting() {
    let temp = tempfile::tempdir().unwrap();
    let store = JournalStore::new(temp.path().join("dcommit-journal"));
    save_initial(
        &store,
        &journal(
            EntryState::Submitted { svn_revision: 41 },
            BatchState::Submitting,
            true,
        ),
    );

    let sink_state = Rc::new(RefCell::new(SinkState::default()));
    let post_state = Rc::new(RefCell::new(PostState::default()));
    let mut restarted = prepared(load(&store));
    let mut coordinator = Coordinator::new(
        sink(&sink_state, [], []),
        RecordingPostSubmit(Rc::clone(&post_state)),
        StorePersistence::acquire(store, None),
    );
    coordinator.run(&mut restarted).unwrap();
    drop(coordinator);

    assert_eq!(sink_state.borrow().remote_checks, 0);
    assert_eq!(sink_state.borrow().submissions, 0);
    assert_eq!(post_state.borrow().fetches, vec![41]);
    let completed = JournalStore::new(temp.path().join("dcommit-journal"))
        .load()
        .unwrap()
        .unwrap();
    assert_eq!(completed.batch_state, BatchState::Complete);
    assert!(matches!(
        completed.entries[0].state,
        EntryState::FetchedVerified {
            svn_revision: 41,
            ..
        }
    ));
}

#[test]
fn submitted_save_acknowledgement_loss_reloads_and_fetches_without_resubmitting() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("dcommit-journal");
    let sink_state = Rc::new(RefCell::new(SinkState::default()));
    let post_state = Rc::new(RefCell::new(PostState::default()));
    let mut first_process = prepared(journal(EntryState::Queued, BatchState::Submitting, true));
    let mut first_coordinator = Coordinator::new(
        sink(
            &sink_state,
            [RemoteHead {
                revision: 40,
                tracking_oid: oid('a'),
            }],
            [41],
        ),
        RecordingPostSubmit(Rc::clone(&post_state)),
        SubmittedAckLossPersistence::acquire(JournalStore::new(&directory)),
    );

    assert!(matches!(
        first_coordinator.run(&mut first_process),
        Err(CoordinatorError::AmbiguousSubmission {
            svn_revision: Some(41),
            ..
        })
    ));
    drop(first_coordinator);
    drop(first_process);
    let persisted = load(&JournalStore::new(&directory));
    assert!(matches!(
        persisted.entries[0].state,
        EntryState::Submitted { svn_revision: 41 }
    ));

    let mut second_process = prepared(persisted);
    let mut second_coordinator = Coordinator::new(
        sink(&sink_state, [], []),
        RecordingPostSubmit(Rc::clone(&post_state)),
        StorePersistence::acquire(JournalStore::new(&directory), None),
    );
    second_coordinator.run(&mut second_process).unwrap();
    drop(second_coordinator);

    assert_eq!(sink_state.borrow().remote_checks, 1);
    assert_eq!(sink_state.borrow().submissions, 1);
    assert_eq!(post_state.borrow().fetches, vec![41]);
    let completed = load(&JournalStore::new(&directory));
    assert_eq!(completed.batch_state, BatchState::Complete);
    assert!(matches!(
        completed.entries[0].state,
        EntryState::FetchedVerified {
            svn_revision: 41,
            ..
        }
    ));
}

#[test]
fn restart_after_fetch_before_verified_save_repeats_only_fetch() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("dcommit-journal");
    let sink_state = Rc::new(RefCell::new(SinkState::default()));
    let post_state = Rc::new(RefCell::new(PostState::default()));
    let mut first_process = prepared(journal(EntryState::Queued, BatchState::Submitting, true));
    let mut first_coordinator = Coordinator::new(
        sink(
            &sink_state,
            [RemoteHead {
                revision: 40,
                tracking_oid: oid('a'),
            }],
            [41],
        ),
        RecordingPostSubmit(Rc::clone(&post_state)),
        StorePersistence::acquire(JournalStore::new(&directory), Some(5)),
    );

    assert!(matches!(
        first_coordinator.run(&mut first_process),
        Err(CoordinatorError::Persistence(message)) if message == "injected save failure 5"
    ));
    drop(first_coordinator);
    drop(first_process);

    let persisted = JournalStore::new(&directory).load().unwrap().unwrap();
    assert!(matches!(
        persisted.entries[0].state,
        EntryState::Submitted { svn_revision: 41 }
    ));

    let mut second_process = prepared(persisted);
    let mut second_coordinator = Coordinator::new(
        sink(&sink_state, [], []),
        RecordingPostSubmit(Rc::clone(&post_state)),
        StorePersistence::acquire(JournalStore::new(&directory), None),
    );
    second_coordinator.run(&mut second_process).unwrap();
    drop(second_coordinator);

    assert_eq!(sink_state.borrow().remote_checks, 1);
    assert_eq!(sink_state.borrow().submissions, 1);
    assert_eq!(post_state.borrow().fetches, vec![41, 41]);
    let completed = JournalStore::new(&directory).load().unwrap().unwrap();
    assert_eq!(completed.batch_state, BatchState::Complete);
    assert!(matches!(
        completed.entries[0].state,
        EntryState::FetchedVerified {
            svn_revision: 41,
            ..
        }
    ));
}

#[test]
fn restart_from_in_flight_snapshot_refuses_to_resubmit() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("dcommit-journal");
    save_initial(
        &JournalStore::new(&directory),
        &journal(
            EntryState::SubmissionInFlight {
                expected_base_revision: 40,
                expected_tracking_oid: oid('a'),
            },
            BatchState::Submitting,
            true,
        ),
    );

    let sink_state = Rc::new(RefCell::new(SinkState::default()));
    let post_state = Rc::new(RefCell::new(PostState::default()));
    let mut restarted = prepared(load(&JournalStore::new(&directory)));
    let mut coordinator = Coordinator::new(
        sink(
            &sink_state,
            [RemoteHead {
                revision: 40,
                tracking_oid: oid('a'),
            }],
            [41],
        ),
        RecordingPostSubmit(Rc::clone(&post_state)),
        StorePersistence::acquire(JournalStore::new(&directory), None),
    );

    assert!(matches!(
        coordinator.run(&mut restarted),
        Err(CoordinatorError::AmbiguousSubmission {
            svn_revision: None,
            ..
        })
    ));
    assert_eq!(sink_state.borrow().remote_checks, 0);
    assert_eq!(sink_state.borrow().submissions, 0);
    assert!(post_state.borrow().fetches.is_empty());
    assert!(matches!(
        load(&JournalStore::new(&directory)).entries[0].state,
        EntryState::SubmissionInFlight { .. }
    ));
}

#[test]
fn restart_from_rebase_pending_retries_without_sink_calls() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("dcommit-journal");
    save_initial(
        &JournalStore::new(&directory),
        &journal(
            EntryState::FetchedVerified {
                svn_revision: 41,
                imported_oid: imported_oid(),
            },
            BatchState::RebasePending,
            false,
        ),
    );

    let sink_state = Rc::new(RefCell::new(SinkState::default()));
    let post_state = Rc::new(RefCell::new(PostState {
        fail_next_rebase: true,
        ..PostState::default()
    }));
    let mut first_process = prepared(load(&JournalStore::new(&directory)));
    let mut first_coordinator = Coordinator::new(
        sink(&sink_state, [], []),
        RecordingPostSubmit(Rc::clone(&post_state)),
        StorePersistence::acquire(JournalStore::new(&directory), None),
    );
    assert!(matches!(
        first_coordinator.run(&mut first_process),
        Err(CoordinatorError::PostSubmit(message)) if message == "injected rebase failure"
    ));
    drop(first_coordinator);
    drop(first_process);

    let persisted = load(&JournalStore::new(&directory));
    assert_eq!(persisted.batch_state, BatchState::RebasePending);
    let mut second_process = prepared(persisted);
    let mut second_coordinator = Coordinator::new(
        sink(&sink_state, [], []),
        RecordingPostSubmit(Rc::clone(&post_state)),
        StorePersistence::acquire(JournalStore::new(&directory), None),
    );
    second_coordinator.run(&mut second_process).unwrap();
    drop(second_coordinator);

    assert_eq!(sink_state.borrow().remote_checks, 0);
    assert_eq!(sink_state.borrow().submissions, 0);
    assert!(post_state.borrow().fetches.is_empty());
    assert_eq!(post_state.borrow().rebases, 2);
    assert_eq!(
        load(&JournalStore::new(&directory)).batch_state,
        BatchState::Complete
    );
}
