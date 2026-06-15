#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChange {
    Modify {
        path: String,
        mode: String,
        content: Vec<u8>,
    },
    Delete {
        path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastImportCommit {
    pub mark: u32,
    pub refname: String,
    pub author: String,
    pub committer: String,
    pub timestamp: i64,
    pub message: String,
    pub parent_mark: Option<u32>,
    pub parent_ref: Option<String>,
    pub changes: Vec<FileChange>,
}

#[derive(Debug, Default)]
pub struct FastImportStream {
    output: Vec<u8>,
}

impl FastImportStream {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn commit(mut self, commit: &FastImportCommit) -> Self {
        self.write_line(&format!("commit {}", commit.refname));
        self.write_line(&format!("mark :{}", commit.mark));
        self.write_line(&format!(
            "author {} {} +0000",
            commit.author, commit.timestamp
        ));
        self.write_line(&format!(
            "committer {} {} +0000",
            commit.committer, commit.timestamp
        ));
        self.write_data(commit.message.as_bytes());

        if let Some(parent_mark) = commit.parent_mark {
            self.write_line(&format!("from :{}", parent_mark));
        } else if let Some(parent_ref) = &commit.parent_ref {
            self.write_line(&format!("from {parent_ref}"));
        }

        for change in &commit.changes {
            match change {
                FileChange::Modify {
                    path,
                    mode,
                    content,
                } => {
                    self.write_line(&format!("M {mode} inline {path}"));
                    self.write_data(content);
                }
                FileChange::Delete { path } => {
                    self.write_line(&format!("D {path}"));
                }
            }
        }

        self.output.push(b'\n');
        self
    }

    pub fn finish(self) -> Vec<u8> {
        self.output
    }

    fn write_data(&mut self, data: &[u8]) {
        self.write_line(&format!("data {}", data.len()));
        self.output.extend_from_slice(data);
        self.output.push(b'\n');
    }

    fn write_line(&mut self, line: &str) {
        self.output.extend_from_slice(line.as_bytes());
        self.output.push(b'\n');
    }
}
