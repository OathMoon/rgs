use crate::cli::DcommitArgs;
use crate::commands::resolver::resolve_tracked_svn;
use crate::dcommit::{GitDiffChange, GitDiffPlanner, PropertyMapper, SvnCommitEditor};
use crate::git::{GitCli, GitCommitSummary};
use crate::rev_map::RevMap;
use crate::svn::CommitRecord;
use crate::svn::mock::MockSvnBackend;

pub fn run(args: DcommitArgs) -> Result<String, String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: DcommitArgs,
) -> Result<String, String> {
    if args.mergeinfo.is_some() && !args.dry_run {
        return Err(
            "--mergeinfo is parsed for compatibility, but mergeinfo write-back is not implemented in v1"
                .to_string(),
        );
    }

    let tracked = resolve_tracked_svn(work_tree)?;
    let revision = tracked
        .max_record()?
        .map(|record| record.revision)
        .unwrap_or(0);
    let target_url = args.commit_url.as_deref().unwrap_or(&tracked.config.url);
    let commits = if tracked.git.rev_parse("HEAD").is_ok() {
        tracked
            .git
            .commit_summaries_between(&tracked.refname, "HEAD")?
    } else {
        Vec::new()
    };

    if !args.dry_run {
        if !target_url.starts_with("mock://") || !tracked.config.url.starts_with("mock://") {
            return Err(
                "dcommit write-back is only implemented for mock:// URLs; production SVN write-back is not implemented"
                    .to_string(),
            );
        }
        return dcommit_mock(
            &tracked.git,
            &tracked.refname,
            tracked.open_rev_map()?,
            &tracked.uuid,
            revision,
            commits,
            args.no_rebase,
        );
    }

    let mut out = format!(
        "Dcommit dry-run against {target_url} ({}, r{revision})\n",
        tracked.refname
    );
    if let Some(mergeinfo) = &args.mergeinfo {
        out.push_str(&format!(
            "explicit mergeinfo accepted for dry-run: {mergeinfo}\n"
        ));
        out.push_str("automatic mergeinfo generation is not implemented in v1\n");
    }
    if commits.is_empty() {
        out.push_str("No local commits to dcommit.\n");
        return Ok(out);
    }

    out.push_str(&format!(
        "Would commit {} local Git commit(s):\n",
        commits.len()
    ));
    for commit in commits {
        out.push_str(&format!("{} {}\n", commit.short_id, commit.subject));
    }
    Ok(out)
}

fn dcommit_mock(
    git: &GitCli,
    refname: &str,
    mut rev_map: RevMap,
    uuid: &str,
    mut base_revision: u32,
    commits: Vec<GitCommitSummary>,
    no_rebase: bool,
) -> Result<String, String> {
    if commits.is_empty() {
        return Ok("No local commits to dcommit.\n".to_string());
    }

    let planner = GitDiffPlanner::new();
    let svn_editor = SvnCommitEditor::new(PropertyMapper);
    let mut backend = MockSvnBackend::new(uuid, Vec::new());
    let mut out = String::new();
    let mut diff_base = refname.to_string();

    for commit in &commits {
        let changes = git_changes_for_commit(git, &diff_base, commit)?;
        let planned = planner.plan(changes)?;
        let record = CommitRecord {
            author: git.commit_author(&commit.id)?,
            message: commit.subject.clone(),
            base_revision,
        };
        let revision = {
            let mut editor = backend.commit_editor(record);
            svn_editor.apply(&mut editor, planned.changes)?
        };
        rev_map.append(revision, &commit.id)?;
        git.update_ref(refname, &commit.id)?;
        base_revision = revision;
        diff_base = commit.id.clone();
        out.push_str(&format!(
            "Committed {} {} as r{revision}\n",
            commit.short_id, commit.subject
        ));
    }

    out.insert_str(
        0,
        &format!("Committed {} local Git commit(s)\n", commits.len()),
    );

    if no_rebase {
        out.push_str("Skipped rebase (--no-rebase).\n");
    } else {
        let rebase_output = git.rebase(refname, false, None)?;
        if rebase_output.is_empty() {
            out.push_str("Rebased onto tracked SVN ref.\n");
        } else {
            out.push_str(&rebase_output);
        }
    }

    Ok(out)
}

fn git_changes_for_commit(
    git: &GitCli,
    base: &str,
    commit: &GitCommitSummary,
) -> Result<Vec<GitDiffChange>, String> {
    git.diff_name_status(base, &commit.id)?
        .into_iter()
        .map(|change| match change.status.as_str() {
            "A" => {
                let content = git.show_file(&commit.id, &change.path)?;
                let mode = git.ls_tree_file(&commit.id, &change.path)?.mode;
                Ok(GitDiffChange::add_file(change.path, content)
                    .with_executable(mode == "100755")
                    .with_symlink(mode == "120000"))
            }
            "M" => {
                let content = git.show_file(&commit.id, &change.path)?;
                let mode = git.ls_tree_file(&commit.id, &change.path)?.mode;
                Ok(GitDiffChange::modify_file(change.path, content)
                    .with_executable(mode == "100755")
                    .with_symlink(mode == "120000"))
            }
            "D" => Ok(GitDiffChange::delete(change.path)),
            status => Err(format!(
                "dcommit does not support git diff status {status} yet"
            )),
        })
        .collect()
}
