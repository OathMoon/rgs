use crate::cli::{CloneArgs, InitArgs};
use crate::commands::{fetch, init};

pub fn run(args: CloneArgs) -> Result<(), String> {
    let path = args.path.clone().unwrap_or_else(|| default_path(&args.url));
    let fetch_args = fetch_args(args.shared.clone());
    let init_args = InitArgs {
        url: args.url,
        path: Some(path.clone()),
        layout: args.layout,
        shared: args.shared,
    };

    init::run(init_args)?;

    fetch::run_in_work_tree(path, fetch_args)
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
