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
    let revision_filter = args
        .revision
        .as_deref()
        .map(parse_revision_filter)
        .transpose()?;
    let git_limit = if revision_filter.is_some() {
        None
    } else {
        args.limit
    };
    let raw = tracked
        .git
        .log_records(&tracked.refname, git_limit, &args.git_log_args)?;
    let formatter = GitSvnLogFormatter::with_incremental(
        args.oneline,
        args.show_commit,
        args.verbose,
        args.incremental,
    );
    let mut out = String::new();
    let mut included = 0_u32;
    for record in raw.split('\x1e') {
        let record = record.trim_matches('\n');
        if record.is_empty() {
            continue;
        }
        let fields = record.splitn(4, '\x1f').collect::<Vec<_>>();
        if fields.len() != 4 {
            continue;
        }
        let Some((id, message)) = split_git_svn_footer(fields[3]) else {
            continue;
        };
        if revision_filter
            .as_ref()
            .is_some_and(|filter| !filter.contains(id.revision))
        {
            continue;
        }
        if args.limit.is_some_and(|limit| included >= limit) {
            break;
        }
        let changed_paths = if args.verbose {
            tracked
                .git
                .commit_name_status(fields[0])?
                .into_iter()
                .map(|change| format!("{}\t{}", change.status, change.path))
                .collect()
        } else {
            Vec::new()
        };
        out.push_str(&formatter.format_entry(&GitSvnLogEntry {
            revision: id.revision,
            author: fields[1].to_string(),
            date: fields[2].to_string(),
            message,
            commit: fields[0].to_string(),
            changed_paths,
        }));
        included += 1;
    }
    if included > 0 && !args.oneline && !args.incremental {
        out.push_str("------------------------------------------------------------------------\n");
    }
    Ok(out)
}

fn split_git_svn_footer(message: &str) -> Option<(GitSvnId, String)> {
    let mut lines = message.lines().collect::<Vec<_>>();
    let footer_index = lines.iter().rposition(|line| !line.trim().is_empty())?;
    let id = GitSvnId::parse(lines[footer_index]).ok()?;

    lines.remove(footer_index);
    if footer_index > 0 && lines[footer_index - 1].trim().is_empty() {
        lines.remove(footer_index - 1);
    }

    Some((id, lines.join("\n")))
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

fn parse_revision_filter(value: &str) -> Result<RevisionFilter, String> {
    if let Some((start, end)) = value.split_once(':') {
        return Ok(RevisionFilter {
            start: parse_optional_revision(start)?,
            end: parse_optional_revision(end)?,
        });
    }

    let revision = parse_revision(value)?;
    Ok(RevisionFilter {
        start: Some(revision),
        end: Some(revision),
    })
}

fn parse_optional_revision(value: &str) -> Result<Option<u32>, String> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    parse_revision(value).map(Some)
}

fn parse_revision(value: &str) -> Result<u32, String> {
    let trimmed = value.trim();
    let revision = trimmed.strip_prefix('r').unwrap_or(trimmed);
    revision
        .parse::<u32>()
        .map_err(|_| format!("invalid SVN revision: {value}"))
}
