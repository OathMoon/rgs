#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSvnLogEntry {
    pub revision: u32,
    pub author: String,
    pub date: String,
    pub message: String,
    pub commit: String,
    pub changed_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitSvnLogFormatter {
    oneline: bool,
    show_commit: bool,
    verbose: bool,
}

impl GitSvnLogFormatter {
    pub fn new(oneline: bool, show_commit: bool, verbose: bool) -> Self {
        Self {
            oneline,
            show_commit,
            verbose,
        }
    }

    pub fn oneline() -> Self {
        Self::new(true, false, false)
    }

    pub fn format_entry(&self, entry: &GitSvnLogEntry) -> String {
        if self.oneline {
            return format!("r{} | {}\n", entry.revision, first_line(&entry.message));
        }

        let mut out = String::new();
        out.push_str("------------------------------------------------------------------------\n");
        out.push_str(&format!(
            "r{} | {} | {} | {} lines\n",
            entry.revision,
            entry.author,
            entry.date,
            line_count(&entry.message)
        ));
        if self.verbose && !entry.changed_paths.is_empty() {
            out.push_str("Changed paths:\n");
            for path in &entry.changed_paths {
                out.push_str("   ");
                out.push_str(path);
                out.push('\n');
            }
        }
        if self.show_commit {
            out.push_str(&format!("commit {}\n", entry.commit));
        }
        out.push('\n');
        out.push_str(entry.message.trim_end());
        out.push('\n');
        out
    }
}

impl Default for GitSvnLogFormatter {
    fn default() -> Self {
        Self::new(false, true, true)
    }
}

fn first_line(message: &str) -> &str {
    message.lines().next().unwrap_or_default()
}

fn line_count(message: &str) -> usize {
    message
        .lines()
        .filter(|line| !line.is_empty())
        .count()
        .max(1)
}
