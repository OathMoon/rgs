use chrono::{Local, TimeZone};
use std::io::IsTerminal;

use crate::authors::{AuthorResolver, parse_authors_file};
use crate::cli::LogArgs;
use crate::commands::resolver::{resolve_tracked_svn, resolve_tracked_svn_at};
use crate::git::GitCli;
use crate::git_svn_id::GitSvnId;
use crate::log_formatter::{GitSvnLogEntry, GitSvnLogFormatter};

pub fn run(args: LogArgs) -> Result<String, String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: LogArgs,
) -> Result<String, String> {
    validate_pager(args.pager.as_deref(), std::io::stdout().is_terminal())?;
    let work_tree = work_tree.into();
    let git = GitCli::new(&work_tree);
    let (treeish, git_log_args) = select_log_treeish(&git, &args.git_log_args);
    let tracked = match treeish.as_deref() {
        Some(treeish) => resolve_tracked_svn_at(&work_tree, treeish)?,
        None => resolve_tracked_svn(&work_tree)?,
    };
    if tracked.config.no_metadata {
        return Err("log is unavailable for --no-metadata imports".to_string());
    }
    let revision_filter = args
        .revision
        .as_deref()
        .map(parse_revision_filter)
        .transpose()?;
    let exact_revision = revision_filter
        .as_ref()
        .and_then(RevisionFilter::exact_revision);
    let log_target = if let Some(filter) = revision_filter.as_ref() {
        tracked
            .records()?
            .into_iter()
            .filter(|record| filter.contains(record.revision) && !record.has_zero_object_id())
            .max_by_key(|record| record.revision)
            .map(|record| record.object_id_hex)
    } else {
        Some(tracked.refname.clone())
    };
    let color = args.color
        || tracked
            .git
            .config_color_bool("color.diff", std::io::stdout().is_terminal())?;
    let raw = match log_target {
        Some(log_target) => tracked.git.log_records(
            &log_target,
            exact_revision.map(|_| 1),
            !args.non_recursive,
            color,
            &git_log_args,
        )?,
        None => String::new(),
    };
    let formatter = GitSvnLogFormatter::with_incremental(
        args.oneline,
        args.show_commit,
        args.verbose,
        args.incremental,
    );
    let authors = load_authors(&tracked, args.authors_file.as_deref())?;
    let mut entries = Vec::new();
    let mut last_revision = None;
    for record in raw.split('\x1e') {
        let record = record.trim_start_matches(['\r', '\n']);
        if record.is_empty() {
            continue;
        }
        let (record, git_output) = record.split_once('\x1d').unwrap_or((record, ""));
        let fields = record.splitn(7, '\x1f').collect::<Vec<_>>();
        if fields.len() != 7 {
            continue;
        }
        let Some((id, message)) = split_git_svn_footer(fields[6]) else {
            continue;
        };
        if last_revision == Some(id.revision) {
            continue;
        }
        last_revision = Some(id.revision);
        if revision_filter
            .as_ref()
            .is_some_and(|filter| !filter.contains(id.revision))
        {
            continue;
        }
        if args
            .limit
            .is_some_and(|limit| entries.len() >= limit as usize)
        {
            break;
        }
        let changed_paths = if args.verbose {
            tracked
                .git
                .commit_name_status(fields[0])?
                .into_iter()
                .filter_map(|change| format_verbose_change(&change))
                .collect()
        } else {
            Vec::new()
        };
        entries.push((
            GitSvnLogEntry {
                revision: id.revision,
                author: svn_author(authors.as_ref(), fields[2], fields[3]),
                date: format_svn_date(fields[4], author_timezone(fields[5])?)?,
                message,
                commit: fields[0].to_string(),
                abbreviated_commit: Some(fields[1].to_string()),
                changed_paths,
            },
            git_output.to_string(),
        ));
    }
    if revision_filter
        .as_ref()
        .is_some_and(|filter| filter.ascending)
    {
        entries.reverse();
    }
    let revision_width = args.oneline.then(|| {
        entries
            .first()
            .map_or(0, |(entry, _)| digits(entry.revision))
    });
    let mut out = String::new();
    for (entry, git_output) in &entries {
        out.push_str(&formatter.format_entry_with_git_output(entry, revision_width, git_output));
    }
    if !args.oneline && !args.incremental {
        out.push_str("------------------------------------------------------------------------\n");
    }
    Ok(out)
}

fn validate_pager(pager: Option<&str>, stdout_is_terminal: bool) -> Result<(), String> {
    if pager.is_some() && stdout_is_terminal {
        return Err("interactive log paging is not implemented in v1".to_string());
    }
    Ok(())
}

fn select_log_treeish(git: &GitCli, args: &[String]) -> (Option<String>, Vec<String>) {
    let mut treeish = None;
    let mut passthrough = Vec::new();
    let mut saw_files = false;
    for arg in args {
        if arg == "--" || saw_files {
            saw_files = true;
            passthrough.push(arg.clone());
        } else if !arg.starts_with('-') && git.rev_parse(&format!("{arg}^0")).is_ok() {
            treeish = Some(arg.clone());
        } else {
            passthrough.push(arg.clone());
        }
    }
    (treeish, passthrough)
}

fn digits(revision: u32) -> usize {
    revision.checked_ilog10().unwrap_or(0) as usize + 1
}

fn load_authors(
    tracked: &crate::commands::resolver::TrackedSvn,
    authors_file: Option<&str>,
) -> Result<Option<AuthorResolver>, String> {
    let Some(path) = authors_file.or(tracked.config.authors_file.as_deref()) else {
        return Ok(None);
    };
    let path = std::path::Path::new(path);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        tracked.git.work_tree().join(path)
    };
    let contents = std::fs::read_to_string(&path)
        .map_err(|error| format!("failed to read authors file {}: {error}", path.display()))?;
    parse_authors_file(&contents).map(Some)
}

fn svn_author(authors: Option<&AuthorResolver>, name: &str, email: &str) -> String {
    authors
        .and_then(|authors| authors.reverse_resolve(name.trim(), email.trim()))
        .map(str::to_string)
        .or_else(|| email.rsplit_once('@').map(|(login, _)| login.to_string()))
        .unwrap_or_else(|| name.trim().to_string())
}

fn author_timezone(author_date: &str) -> Result<&str, String> {
    author_date
        .split_whitespace()
        .next_back()
        .ok_or_else(|| format!("invalid Git author date: {author_date}"))
}

fn format_svn_date(epoch: &str, timezone: &str) -> Result<String, String> {
    let epoch = epoch
        .parse::<i64>()
        .map_err(|_| format!("invalid Git author timestamp: {epoch}"))?;
    let epoch = epoch
        .checked_add(parse_timezone_offset(timezone)?)
        .ok_or_else(|| format!("Git author timestamp is out of range: {epoch} {timezone}"))?;
    let date = Local
        .timestamp_opt(epoch, 0)
        .single()
        .ok_or_else(|| format!("Git author timestamp is out of range: {epoch}"))?;
    Ok(date
        .format("%Y-%m-%d %H:%M:%S %z (%a, %d %b %Y)")
        .to_string())
}

fn parse_timezone_offset(timezone: &str) -> Result<i64, String> {
    let bytes = timezone.as_bytes();
    if bytes.len() != 5
        || !matches!(bytes[0], b'+' | b'-')
        || !bytes[1..].iter().all(u8::is_ascii_digit)
    {
        return Err(format!("invalid Git author timezone: {timezone}"));
    }
    let hours = timezone[1..3]
        .parse::<i64>()
        .map_err(|_| format!("invalid Git author timezone: {timezone}"))?;
    let minutes = timezone[3..5]
        .parse::<i64>()
        .map_err(|_| format!("invalid Git author timezone: {timezone}"))?;
    if hours > 23 || minutes > 59 {
        return Err(format!("invalid Git author timezone: {timezone}"));
    }
    let seconds = hours * 3600 + minutes * 60;
    Ok(if bytes[0] == b'-' { -seconds } else { seconds })
}

fn format_verbose_change(change: &crate::git::GitNameStatus) -> Option<String> {
    // Frozen Log.pm only recognizes one-letter name-status records. Git emits
    // scored rename/copy records (for example R100), which it leaves out.
    matches!(change.status.as_str(), "A" | "C" | "R" | "M" | "D" | "T")
        .then(|| format!("{} {}", change.status, change.path))
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
        message_end = preceding_line_start;
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
    ascending: bool,
}

impl RevisionFilter {
    fn contains(&self, revision: u32) -> bool {
        self.start.is_none_or(|start| revision >= start)
            && self.end.is_none_or(|end| revision <= end)
    }

    fn exact_revision(&self) -> Option<u32> {
        (self.start == self.end).then_some(self.start).flatten()
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
                ascending: false,
            });
        }
        return Ok(RevisionFilter {
            start,
            end,
            ascending: matches!((start, end), (Some(start), Some(end)) if start < end),
        });
    }

    let revision = parse_revision(value)?;
    Ok(RevisionFilter {
        start: Some(revision),
        end: Some(revision),
        ascending: false,
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
    use super::{
        format_svn_date, parse_timezone_offset, split_git_svn_footer, svn_author, validate_pager,
    };
    use crate::authors::parse_authors_file;

    const FOOTER: &str = "git-svn-id: mock://repo/trunk@2 mock-uuid";

    #[test]
    fn pager_is_a_noop_off_tty_and_explicit_at_tty_boundary() {
        assert!(validate_pager(Some("anything"), false).is_ok());
        assert_eq!(
            validate_pager(Some("cat"), true).unwrap_err(),
            "interactive log paging is not implemented in v1"
        );
        assert!(validate_pager(None, true).is_ok());
    }

    #[test]
    fn footer_split_preserves_crlf_body() {
        let (_, message) =
            split_git_svn_footer(&format!("subject\r\n\r\nbody\r\n\r\n{FOOTER}\r\n")).unwrap();

        assert_eq!(message, "subject\r\n\r\nbody\r\n");
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

        assert_eq!(message, "subject\n");
    }

    #[test]
    fn footer_split_removes_only_one_of_multiple_preceding_blank_lines() {
        let (_, message) = split_git_svn_footer(&format!("subject\n\n\n{FOOTER}")).unwrap();

        assert_eq!(message, "subject\n\n");
    }

    #[test]
    fn author_uses_reverse_mapping_then_email_login_fallback() {
        let authors = parse_authors_file("svn-alice = Alice Example <dev@example.com>\n").unwrap();

        assert_eq!(
            svn_author(Some(&authors), "Alice Example", "dev@example.com"),
            "svn-alice"
        );
        assert_eq!(svn_author(None, "Alice Example", "dev@example.com"), "dev");
        assert_eq!(
            svn_author(None, "Alice Example", "invalid"),
            "Alice Example"
        );
    }

    #[test]
    fn date_uses_frozen_svn_log_shape() {
        let formatted = format_svn_date("1767225600", "+0000").unwrap();

        assert!(formatted.starts_with("2026-01-01 "));
        assert!(formatted.ends_with("(Thu, 01 Jan 2026)"));
        assert_eq!(formatted.as_bytes()[19], b' ');
        assert!(matches!(formatted.as_bytes()[20], b'+' | b'-'));
    }

    #[test]
    fn author_timezone_offset_matches_frozen_log_pm_adjustment() {
        assert_eq!(parse_timezone_offset("+0800").unwrap(), 8 * 3600);
        assert_eq!(
            parse_timezone_offset("-0530").unwrap(),
            -(5 * 3600 + 30 * 60)
        );
        assert!(parse_timezone_offset("+2460").is_err());
    }
}
