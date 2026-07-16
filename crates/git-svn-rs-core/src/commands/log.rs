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
    if tracked.config.no_metadata {
        return Err("log is unavailable for --no-metadata imports".to_string());
    }
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
    let verbose_path_prefix = verbose_path_prefix(&tracked.svn_path, &tracked.config.url);
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
                .map(|change| format_verbose_change(&verbose_path_prefix, &change))
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

fn verbose_path_prefix(svn_path: &str, url: &str) -> String {
    let svn_path = svn_path.trim_matches('/');
    if !svn_path.is_empty() {
        return svn_path.to_string();
    }
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|segment| !segment.contains("://"))
        .unwrap_or("")
        .to_string()
}

fn format_verbose_change(svn_path: &str, change: &crate::git::GitNameStatus) -> String {
    let status = change.status.chars().next().unwrap_or('M');
    let path = svn_log_path(svn_path, &change.path);
    if let Some(old_path) = &change.old_path {
        return format!(
            "{status} {path} (from {})",
            svn_log_path(svn_path, old_path)
        );
    }
    format!("{status} {path}")
}

fn svn_log_path(prefix: &str, path: &str) -> String {
    match (prefix.trim_matches('/'), path.trim_start_matches('/')) {
        ("", "") => "/".to_string(),
        ("", path) => format!("/{path}"),
        (prefix, "") => format!("/{prefix}"),
        (prefix, path) => format!("/{prefix}/{path}"),
    }
}

fn split_git_svn_footer(message: &str) -> Option<(GitSvnId, String)> {
    let footer_end = message.trim_end_matches(char::is_whitespace).len();
    let footer_start = message[..footer_end]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let id = GitSvnId::parse(&message[footer_start..footer_end]).ok()?;

    let mut message_end = line_ending_start_before(message, footer_start).unwrap_or(footer_start);
    let preceding_line_start = message[..message_end]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    if message[preceding_line_start..message_end].trim().is_empty() {
        message_end = line_ending_start_before(message, preceding_line_start).unwrap_or(0);
    }

    Some((id, message[..message_end].to_string()))
}

fn line_ending_start_before(message: &str, end: usize) -> Option<usize> {
    if message[..end].ends_with("\r\n") {
        Some(end - 2)
    } else if message[..end].ends_with('\n') {
        Some(end - 1)
    } else {
        None
    }
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
        let start = parse_optional_revision(start)?;
        let end = parse_optional_revision(end)?;
        if let (Some(start), Some(end)) = (start, end)
            && start > end
        {
            return Ok(RevisionFilter {
                start: Some(end),
                end: Some(start),
            });
        }
        return Ok(RevisionFilter { start, end });
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

#[cfg(test)]
mod tests {
    use super::split_git_svn_footer;

    const FOOTER: &str = "git-svn-id: mock://repo/trunk@2 mock-uuid";

    #[test]
    fn footer_split_preserves_crlf_body() {
        let (_, message) =
            split_git_svn_footer(&format!("subject\r\n\r\nbody\r\n\r\n{FOOTER}\r\n")).unwrap();

        assert_eq!(message, "subject\r\n\r\nbody");
    }

    #[test]
    fn footer_split_handles_footer_only_message() {
        let (id, message) = split_git_svn_footer(FOOTER).unwrap();

        assert_eq!(id.revision, 2);
        assert_eq!(message, "");
    }

    #[test]
    fn footer_split_ignores_all_whitespace_after_footer() {
        let (_, message) = split_git_svn_footer(&format!("subject\n\n{FOOTER}\n \n\t\n")).unwrap();

        assert_eq!(message, "subject");
    }

    #[test]
    fn footer_split_removes_only_one_of_multiple_preceding_blank_lines() {
        let (_, message) = split_git_svn_footer(&format!("subject\n\n\n{FOOTER}")).unwrap();

        assert_eq!(message, "subject\n");
    }
}
