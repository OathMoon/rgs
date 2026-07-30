use crate::cli::{CloneArgs, InitArgs};
use crate::commands::{fetch, init};
use crate::git::GitCli;
use crate::git_svn_id::GitSvnId;
use crate::mapping::build_from_layout_args;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneOutput {
    pub stdout: String,
    pub stderr: String,
}

pub fn run(args: CloneArgs) -> Result<(), String> {
    run_with_output(args).map(|_| ())
}

pub fn run_with_output(mut args: CloneArgs) -> Result<CloneOutput, String> {
    let path = args.path.clone().unwrap_or_else(|| default_path(&args.url));
    build_from_layout_args(
        args.layout.stdlayout,
        args.layout.trunk.as_deref(),
        &args.layout.branches,
        &args.layout.tags,
        args.layout.prefix.as_deref(),
    )?;
    let normalization_notice =
        init::normalize_layout_args(&mut args.url, &mut args.layout, &args.shared)?;
    let mappings = build_from_layout_args(
        args.layout.stdlayout,
        args.layout.trunk.as_deref(),
        &args.layout.branches,
        &args.layout.tags,
        args.layout.prefix.as_deref(),
    )?;
    let primary_ref = mappings
        .fetch
        .first()
        .map(|mapping| mapping.git_ref.clone());
    let fetch_args = fetch_args(args.shared.clone());
    let no_checkout = args.no_checkout;
    let no_metadata = args.shared.no_metadata;
    let placeholder = args.shared.placeholder_filename.clone();
    let preserve_empty_dirs = args.shared.preserve_empty_dirs;
    let mapping_path = mappings
        .fetch
        .first()
        .map(|mapping| mapping.svn_path.clone())
        .unwrap_or_default();
    let mut init_shared = args.shared;
    init_shared.revision = None;
    init_shared.password = None;
    let init_args = InitArgs {
        url: args.url,
        path: Some(path.clone()),
        layout: args.layout,
        shared: init_shared,
    };

    let mut init_output = init::run_prepared_with_output(init_args, mappings)?;
    if let Some(notice) = normalization_notice {
        init_output.stderr.push_str(&notice);
    }

    fetch::run_in_work_tree(&path, fetch_args)?;
    let git = GitCli::new(&path);
    let mut stdout = init_output.stdout;
    let mut stderr = init_output.stderr;
    if let Some(primary_ref) = primary_ref {
        if !no_metadata {
            append_import_progress(
                &git,
                &mapping_path,
                preserve_empty_dirs.then_some(placeholder.as_str()),
                &mut stdout,
                &mut stderr,
            )?;
        }
        git.materialize_initial_branch(&primary_ref, no_checkout)?;
        if !no_checkout && !no_metadata {
            let tip_message = git.commit_message(&primary_ref)?;
            let footer = tip_message
                .lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .ok_or_else(|| "imported clone tip has no git-svn-id footer".to_string())?;
            let id = GitSvnId::parse(footer)?;
            stderr.push_str(&format!(
                "Checked out HEAD:\n  {} r{}\n",
                id.url, id.revision
            ));
        }
    }
    Ok(CloneOutput { stdout, stderr })
}

fn append_import_progress(
    git: &GitCli,
    primary_mapping_path: &str,
    placeholder: Option<&str>,
    stdout: &mut String,
    stderr: &mut String,
) -> Result<(), String> {
    let mut progress = Vec::new();
    let mut seen = BTreeSet::new();
    for refname in git.refs_under("refs/remotes")? {
        if refname.ends_with("/HEAD") {
            continue;
        }
        let tip_id = commit_git_svn_id(git, &refname)?;
        let mut history = git.first_parent_history(&refname)?;
        history.reverse();
        let mut first_native = true;
        for commit in history {
            let id = commit_git_svn_id(git, &commit)?;
            if id.url != tip_id.url || !seen.insert((refname.clone(), commit.clone())) {
                continue;
            }
            progress.push((id.revision, refname.clone(), commit, id, first_native));
            first_native = false;
        }
    }
    progress.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

    for (_revision, refname, commit, id, first_native) in progress {
        let parent = git.rev_parse(&format!("{commit}^")).ok();
        let copy_parent = if first_native {
            match parent
                .as_deref()
                .map(str::trim)
                .filter(|parent| !parent.is_empty())
            {
                Some(parent) => {
                    let parent_id = commit_git_svn_id(git, parent)?;
                    (parent_id.url != id.url).then(|| (parent.to_string(), parent_id))
                }
                None => None,
            }
        } else {
            None
        };

        if let Some((parent, parent_id)) = &copy_parent {
            stderr.push_str(&format!(
                "Found possible branch point: {} => {}, {}\n",
                parent_id.url, id.url, parent_id.revision
            ));
            stderr.push_str(&format!("Found branch parent: ({refname}) {parent}\n"));
            stderr.push_str("Following parent with do_switch\nSuccessfully followed parent\n");
        }

        if copy_parent.is_some() {
            for file in git.tree_files(&commit)? {
                if placeholder.is_some_and(|name| file.path.ends_with(&format!("/{name}"))) {
                    continue;
                }
                stdout.push_str(&format!("\tA\t{}\n", file.path));
            }
        } else {
            for change in git.commit_name_status(&commit)? {
                if placeholder.is_some_and(|name| change.path.ends_with(&format!("/{name}"))) {
                    continue;
                }
                let status = change.status.chars().next().unwrap_or('M');
                stdout.push_str(&format!("\t{status}\t{}\n", change.path));
                if status == 'D' {
                    let path = match primary_mapping_path.trim_matches('/') {
                        "" => change.path.clone(),
                        prefix => format!("{prefix}/{}", change.path),
                    };
                    stderr.push_str(&format!("W: -empty_dir: {path}\n"));
                }
            }
        }
        stdout.push_str(&format!("r{} = {commit} ({refname})\n", id.revision));
    }
    Ok(())
}

fn commit_git_svn_id(git: &GitCli, commit: &str) -> Result<GitSvnId, String> {
    let message = git.commit_message(commit)?;
    let footer = message
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| format!("imported commit {commit} has no git-svn-id footer"))?;
    GitSvnId::parse(footer)
}

fn fetch_args(shared: crate::cli::SharedFetchArgs) -> crate::cli::FetchArgs {
    crate::cli::FetchArgs {
        remote: None,
        shared,
        fetch_all: false,
        parent: false,
    }
}

fn default_path(url: &str) -> String {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("repo")
        .to_string()
}
