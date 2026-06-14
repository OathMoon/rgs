use crate::cli::{CloneArgs, InitArgs};
use crate::commands::{fetch, init};

pub fn run(args: CloneArgs) -> Result<(), String> {
    let path = args.path.clone().unwrap_or_else(|| default_path(&args.url));
    let init_args = InitArgs {
        url: args.url,
        path: Some(path.clone()),
        layout: args.layout,
        shared: args.shared,
    };

    init::run(init_args)?;

    fetch::run_in_work_tree(path, default_fetch_args())
}

fn default_fetch_args() -> crate::cli::FetchArgs {
    crate::cli::FetchArgs {
        remote: None,
        shared: crate::cli::SharedFetchArgs {
            authors_file: None,
            authors_prog: None,
            ignore_paths: None,
            include_paths: None,
            ignore_refs: None,
            revision: None,
            log_window_size: None,
            localtime: false,
            no_metadata: false,
            rewrite_root: None,
            rewrite_uuid: None,
            username: None,
            config_dir: None,
            no_auth_cache: false,
        },
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
