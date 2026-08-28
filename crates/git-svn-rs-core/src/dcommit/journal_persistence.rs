use super::coordinator::JournalPersistence;
use super::journal::{DcommitJournal, JournalError, JournalLock, JournalStore};

/// Durable coordinator persistence that keeps the journal lock for its lifetime.
pub struct JournalStorePersistence {
    store: JournalStore,
    _lock: JournalLock,
    last_generation: Option<u64>,
}

impl JournalStorePersistence {
    pub fn new(store: JournalStore) -> Result<Self, JournalError> {
        let lock = store.acquire_lock()?;
        Ok(Self {
            store,
            _lock: lock,
            last_generation: None,
        })
    }

    #[cfg(test)]
    pub fn load(&self) -> Result<Option<DcommitJournal>, JournalError> {
        self.store.load()
    }

    #[cfg(test)]
    pub fn last_generation(&self) -> Option<u64> {
        self.last_generation
    }
}

impl JournalPersistence for JournalStorePersistence {
    fn persist(&mut self, journal: &DcommitJournal) -> Result<(), String> {
        let generation = self
            .store
            .save(&self._lock, journal)
            .map_err(|error| error.to_string())?;
        self.last_generation = Some(generation);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcommit::journal::{BatchState, DcommitTargetIdentity, EntryState, JournalEntry};

    fn oid(character: char) -> String {
        character.to_string().repeat(40)
    }

    fn journal() -> DcommitJournal {
        DcommitJournal {
            target: DcommitTargetIdentity {
                remote_id: "svn".to_owned(),
                repository_root_url: "https://example.invalid/repos/project".to_owned(),
                repository_uuid: "12345678-1234-1234-1234-123456789abc".to_owned(),
                mapping_ref: "refs/remotes/origin/trunk".to_owned(),
                rev_map_path: ".git/svn/refs/remotes/origin/trunk/.rev_map.uuid".to_owned(),
                commit_url: "https://example.invalid/repos/project/trunk".to_owned(),
            },
            original_base_revision: 40,
            original_base_oid: oid('a'),
            original_head: oid('b'),
            no_rebase: true,
            config_fingerprint: "1010".to_owned(),
            entries: vec![JournalEntry {
                git_oid: oid('b'),
                base_oid: oid('a'),
                plan_fingerprint: "2020".to_owned(),
                message_fingerprint: "3030".to_owned(),
                state: EntryState::Queued,
            }],
            batch_state: BatchState::Submitting,
        }
    }

    #[test]
    fn persists_consecutive_generations_while_holding_the_store_lock() {
        let temp = tempfile::tempdir().unwrap();
        let store = JournalStore::new(temp.path());
        let mut persistence = JournalStorePersistence::new(store.clone()).unwrap();

        persistence.persist(&journal()).unwrap();
        assert_eq!(persistence.last_generation(), Some(1));
        persistence.persist(&journal()).unwrap();
        assert_eq!(persistence.last_generation(), Some(2));
        assert_eq!(persistence.load().unwrap(), Some(journal()));
        assert!(matches!(
            store.acquire_lock(),
            Err(JournalError::LockHeld(_))
        ));

        drop(persistence);
        store.acquire_lock().unwrap();
    }
}
