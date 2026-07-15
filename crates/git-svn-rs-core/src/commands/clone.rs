use crate::cli::{CloneArgs, InitArgs};
use crate::commands::{fetch, init};
use crate::git::GitCli;
use crate::mapping::build_from_layout_args;

pub fn run(args: CloneArgs) -> Result<(), String> {
    let path = args.path.clone().unwrap_or_else(|| default_path(&args.url));
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
    let mut init_shared = args.shared;
    init_shared.revision = None;
    init_shared.password = None;
    let init_args = InitArgs {
        url: args.url,
        path: Some(path.clone()),
        layout: args.layout,
        shared: init_shared,
    };

    init::run(init_args)?;

    fetch::run_in_work_tree(&path, fetch_args)?;
    if let Some(primary_ref) = primary_ref {
        GitCli::new(path).materialize_initial_branch(&primary_ref, no_checkout)?;
    }
    Ok(())
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
