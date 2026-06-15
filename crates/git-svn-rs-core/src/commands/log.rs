use crate::cli::LogArgs;
use crate::commands::resolver::resolve_tracked_svn;
use crate::git_svn_id::GitSvnId;
use crate::log_formatter::{GitSvnLogEntry, GitSvnLogFormatter};

pub fn run(args: LogArgs) -> Result<String, String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: LogArgs,
) -> Result<String, String> {
    let tracked = resolve_tracked_svn(work_tree)?;
    let raw = tracked.git.log_records(&tracked.refname, args.limit)?;
    let formatter = GitSvnLogFormatter::new(args.oneline, args.show_commit, args.verbose);
    let revision_filter = args.revision.as_deref().and_then(parse_revision_filter);
    let mut out = String::new();
    for record in raw.split('\x1e') {
        let record = record.trim_matches('\n');
        if record.is_empty() {
            continue;
        }
        let fields = record.splitn(4, '\x1f').collect::<Vec<_>>();
        if fields.len() != 4 {
            continue;
        }
        let Some(id) = fields[3]
            .lines()
            .find_map(|line| GitSvnId::parse(line).ok())
        else {
            continue;
        };
        if revision_filter
            .as_ref()
            .is_some_and(|filter| !filter.contains(id.revision))
        {
            continue;
        }
        let message = strip_git_svn_footer(fields[3]);
        out.push_str(&formatter.format_entry(&GitSvnLogEntry {
            revision: id.revision,
            author: fields[1].to_string(),
            date: fields[2].to_string(),
            message,
            commit: fields[0].to_string(),
            changed_paths: Vec::new(),
        }));
    }
    Ok(out)
}

fn strip_git_svn_footer(message: &str) -> String {
    message
        .lines()
        .filter(|line| GitSvnId::parse(line).is_err())
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

struct RevisionFilter {
    start: Option<u32>,
    end: Option<u32>,
}

impl RevisionFilter {
    fn contains(&self, revision: u32) -> bool {
        self.start.is_none_or(|start| revision >= start)
            && self.end.is_none_or(|end| revision <= end)
    }
}

fn parse_revision_filter(value: &str) -> Option<RevisionFilter> {
    if let Some((start, end)) = value.split_once(':') {
        return Some(RevisionFilter {
            start: parse_optional_revision(start)?,
            end: parse_optional_revision(end)?,
        });
    }

    let revision = parse_revision(value)?;
    Some(RevisionFilter {
        start: Some(revision),
        end: Some(revision),
    })
}

fn parse_optional_revision(value: &str) -> Option<Option<u32>> {
    if value.trim().is_empty() {
        return Some(None);
    }
    parse_revision(value).map(Some)
}

fn parse_revision(value: &str) -> Option<u32> {
    value
        .trim()
        .strip_prefix('r')
        .unwrap_or(value.trim())
        .parse::<u32>()
        .ok()
}
