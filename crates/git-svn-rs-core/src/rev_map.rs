use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectFormat {
    Sha1,
    Sha256,
}

impl ObjectFormat {
    pub fn object_bytes(self) -> usize {
        match self {
            ObjectFormat::Sha1 => 20,
            ObjectFormat::Sha256 => 32,
        }
    }

    pub fn hex_len(self) -> usize {
        self.object_bytes() * 2
    }

    pub fn record_size(self) -> u64 {
        4 + self.object_bytes() as u64
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevMapRecord {
    pub revision: u32,
    pub object_id_hex: String,
}

impl RevMapRecord {
    fn has_zero_object_id(&self) -> bool {
        self.object_id_hex.chars().all(|c| c == '0')
    }
}

pub struct RevMap {
    path: PathBuf,
    format: ObjectFormat,
}

impl RevMap {
    pub fn open(path: impl AsRef<Path>, format: ObjectFormat) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        Ok(Self { path, format })
    }

    pub fn append(&mut self, revision: u32, object_id_hex: &str) -> Result<(), String> {
        if object_id_hex.len() != self.format.hex_len() {
            return Err(format!(
                "object id must be {} hex chars",
                self.format.hex_len()
            ));
        }
        let raw = hex::decode(object_id_hex).map_err(|e| e.to_string())?;
        let mut record = Vec::with_capacity(self.format.record_size() as usize);
        record.extend_from_slice(&revision.to_be_bytes());
        record.extend_from_slice(&raw);
        let _lock = RevMapLock::acquire(&self.path)?;
        if let Some(last) = self.max_record(false)?
            && revision <= last.revision
        {
            return Err(format!(
                "out-of-order .rev_map append: revision {revision} after {}",
                last.revision
            ));
        }
        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.path)
            .map_err(|e| e.to_string())?;
        file.write_all(&record).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get(&self, revision: u32) -> Result<Option<String>, String> {
        let mut file = File::open(&self.path).map_err(|e| e.to_string())?;
        let size = self.validated_size(&file)?;
        let record_size = self.format.record_size();
        let mut low = 0_i64;
        let mut high = (size / record_size) as i64 - 1;

        while low <= high {
            let mid = (low + high) / 2;
            file.seek(SeekFrom::Start(mid as u64 * record_size))
                .map_err(|e| e.to_string())?;
            let record = self.read_record(&mut file)?;
            match record.revision.cmp(&revision) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid - 1,
                std::cmp::Ordering::Equal => {
                    if record.has_zero_object_id() {
                        return Ok(None);
                    }
                    return Ok(Some(record.object_id_hex));
                }
            }
        }
        Ok(None)
    }

    pub fn max_revision(&self, require_commit: bool) -> Result<Option<u32>, String> {
        Ok(self
            .max_record(require_commit)?
            .map(|record| record.revision))
    }

    pub fn max_record(&self, want_commit: bool) -> Result<Option<RevMapRecord>, String> {
        let mut file = File::open(&self.path).map_err(|e| e.to_string())?;
        let size = self.validated_size(&file)?;
        let record_size = self.format.record_size();
        if size == 0 {
            return Ok(None);
        }

        let last_index = size / record_size - 1;
        let last = self.read_record_at(&mut file, last_index)?;
        if !want_commit || !last.has_zero_object_id() {
            return Ok(Some(last));
        }
        if last_index == 0 {
            return Ok(None);
        }

        let penultimate = self.read_record_at(&mut file, last_index - 1)?;
        if penultimate.has_zero_object_id() {
            return Err("inconsistent .rev_map: multiple trailing all-zero records".to_string());
        }
        Ok(Some(penultimate))
    }

    pub fn reset_to(&mut self, revision: u32, object_id_hex: &str) -> Result<(), String> {
        let _lock = RevMapLock::acquire(&self.path)?;
        let (records_to_keep, found) = self.find_record_position(revision)?;
        match found {
            Some(found) if found.object_id_hex == object_id_hex => {
                let file = OpenOptions::new()
                    .write(true)
                    .open(&self.path)
                    .map_err(|e| e.to_string())?;
                file.set_len((records_to_keep + 1) * self.format.record_size())
                    .map_err(|e| e.to_string())?;
                file.sync_all().map_err(|e| e.to_string())?;
                Ok(())
            }
            Some(found) if found.has_zero_object_id() => {
                Err(format!("revision {revision} not found"))
            }
            Some(found) => Err(format!(
                "revision {revision} maps to {}, not {object_id_hex}",
                found.object_id_hex
            )),
            None => Err(format!("revision {revision} not found")),
        }
    }

    fn find_record_position(&self, revision: u32) -> Result<(u64, Option<RevMapRecord>), String> {
        let mut file = File::open(&self.path).map_err(|e| e.to_string())?;
        let size = self.validated_size(&file)?;
        for index in 0..(size / self.format.record_size()) {
            let record = self.read_record_at(&mut file, index)?;
            if record.revision == revision {
                return Ok((index, Some(record)));
            }
        }
        Ok((0, None))
    }

    fn read_record_at(&self, file: &mut File, index: u64) -> Result<RevMapRecord, String> {
        file.seek(SeekFrom::Start(index * self.format.record_size()))
            .map_err(|e| e.to_string())?;
        self.read_record(file)
    }

    fn read_record(&self, file: &mut File) -> Result<RevMapRecord, String> {
        let mut rev = [0_u8; 4];
        file.read_exact(&mut rev).map_err(|e| e.to_string())?;
        let mut oid = vec![0_u8; self.format.object_bytes()];
        file.read_exact(&mut oid).map_err(|e| e.to_string())?;
        Ok(RevMapRecord {
            revision: u32::from_be_bytes(rev),
            object_id_hex: hex::encode(oid),
        })
    }

    fn validated_size(&self, file: &File) -> Result<u64, String> {
        let size = file.metadata().map_err(|e| e.to_string())?.len();
        let record_size = self.format.record_size();
        if size % record_size != 0 {
            Err(format!("inconsistent .rev_map size: {size}"))
        } else {
            Ok(size)
        }
    }
}

struct RevMapLock {
    path: PathBuf,
}

impl RevMapLock {
    fn acquire(rev_map_path: &Path) -> Result<Self, String> {
        let path = lock_path(rev_map_path);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                file.sync_all().map_err(|e| e.to_string())?;
                Ok(Self { path })
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(format!("rev_map lock exists: {}", path.display()))
            }
            Err(err) => Err(err.to_string()),
        }
    }
}

impl Drop for RevMapLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn lock_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(".rev_map");
    path.with_file_name(format!("{file_name}.lock"))
}
