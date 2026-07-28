#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSvnLogEntry {
    pub revision: u32,
    pub author: String,
    pub date: String,
    pub message: String,
    pub commit: String,
    pub abbreviated_commit: Option<String>,
    pub changed_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitSvnLogFormatter {
    oneline: bool,
    show_commit: bool,
    verbose: bool,
    incremental: bool,
}

const SEPARATOR: &str = "------------------------------------------------------------------------";

impl GitSvnLogFormatter {
    pub fn new(oneline: bool, show_commit: bool, verbose: bool) -> Self {
        Self::with_incremental(oneline, show_commit, verbose, false)
    }

    pub fn with_incremental(
        oneline: bool,
        show_commit: bool,
        verbose: bool,
        incremental: bool,
    ) -> Self {
        Self {
            oneline,
            show_commit,
            verbose,
            incremental,
        }
    }

    pub fn oneline() -> Self {
        Self::new(true, false, false)
    }

    pub fn incremental(show_commit: bool, verbose: bool) -> Self {
        Self::with_incremental(false, show_commit, verbose, true)
    }

    pub fn format_entry(&self, entry: &GitSvnLogEntry) -> String {
        self.format_entry_with_revision_width(entry, None)
    }

    pub fn format_entry_with_revision_width(
        &self,
        entry: &GitSvnLogEntry,
        revision_width: Option<usize>,
    ) -> String {
        self.format_entry_with_git_output(entry, revision_width, "")
    }

    pub fn format_entry_with_git_output(
        &self,
        entry: &GitSvnLogEntry,
        revision_width: Option<usize>,
        git_output: &str,
    ) -> String {
        if self.oneline {
            let revision = match revision_width {
                Some(width) => format!("{:<width$}", entry.revision),
                None => entry.revision.to_string(),
            };
            if self.show_commit {
                return format!(
                    "r{revision} | {} | {}\n",
                    entry
                        .abbreviated_commit
                        .as_deref()
                        .unwrap_or_else(|| short_commit(&entry.commit)),
                    first_line(&entry.message)
                );
            }
            return format!("r{revision} | {}\n", first_line(&entry.message));
        }

        let message = collapse_trailing_blank_lines(&entry.message);
        let mut out = String::new();
        out.push_str(SEPARATOR);
        out.push('\n');
        let line_count = line_count(message);
        let line_label = if line_count == 1 { "line" } else { "lines" };
        out.push_str(&format!("r{} | ", entry.revision));
        if self.show_commit {
            out.push_str(&format!(
                "{} | ",
                entry
                    .abbreviated_commit
                    .as_deref()
                    .unwrap_or_else(|| short_commit(&entry.commit))
            ));
        }
        out.push_str(&format!(
            "{} | {} | {line_count} {line_label}\n",
            entry.author, entry.date
        ));
        if self.verbose && !entry.changed_paths.is_empty() {
            out.push_str("Changed paths:\n");
            for path in &entry.changed_paths {
                out.push_str("   ");
                out.push_str(path);
                out.push('\n');
            }
        }
        out.push('\n');
        out.push_str(message);
        if !message.ends_with('\n') {
            out.push('\n');
        }
        let git_output = git_output.trim_matches(['\r', '\n']);
        if !git_output.is_empty() {
            out.push('\n');
            out.push_str(git_output);
            out.push('\n');
        }
        out
    }
}

impl Default for GitSvnLogFormatter {
    fn default() -> Self {
        Self::new(false, true, true)
    }
}

fn first_line(message: &str) -> &str {
    message
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
}

fn short_commit(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}

fn line_count(message: &str) -> usize {
    if message.bytes().all(|byte| byte == b'\n') {
        1
    } else {
        message.split('\n').count().max(1)
    }
}

fn collapse_trailing_blank_lines(message: &str) -> &str {
    let trailing_newlines = message
        .as_bytes()
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\n')
        .count();
    let retained = trailing_newlines.min(1);
    &message[..message.len() - (trailing_newlines - retained)]
}
