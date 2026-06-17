use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "git-svn-rs",
    version,
    about = "Rust replacement for core git-svn workflows"
)]
pub struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[arg(short, long)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Init(InitArgs),
    Clone(CloneArgs),
    Fetch(FetchArgs),
    Rebase(RebaseArgs),
    Dcommit(DcommitArgs),
    Log(LogArgs),
    Info(InfoArgs),
    #[command(name = "find-rev")]
    FindRev(FindRevArgs),
    Gc(GcArgs),
    Reset(ResetArgs),
    Diagnose(DiagnoseArgs),
    Branch(UnsupportedArgs),
    Tag(UnsupportedArgs),
    #[command(name = "set-tree")]
    SetTree(UnsupportedArgs),
    Propget(UnsupportedArgs),
    Propset(UnsupportedArgs),
    Proplist(UnsupportedArgs),
    #[command(name = "show-ignore")]
    ShowIgnore(UnsupportedArgs),
    #[command(name = "show-externals")]
    ShowExternals(UnsupportedArgs),
    #[command(external_subcommand)]
    Unsupported(Vec<String>),
}

#[derive(Debug, Args)]
pub struct LayoutArgs {
    #[arg(short = 's', long)]
    pub stdlayout: bool,
    #[arg(short = 'T', long)]
    pub trunk: Option<String>,
    #[arg(short = 'b', long)]
    pub branches: Vec<String>,
    #[arg(short = 't', long)]
    pub tags: Vec<String>,
    #[arg(long)]
    pub prefix: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct SharedFetchArgs {
    #[arg(short = 'A', long = "authors-file")]
    pub authors_file: Option<String>,
    #[arg(long = "authors-prog")]
    pub authors_prog: Option<String>,
    #[arg(long = "ignore-paths")]
    pub ignore_paths: Option<String>,
    #[arg(long = "include-paths")]
    pub include_paths: Option<String>,
    #[arg(long = "ignore-refs")]
    pub ignore_refs: Option<String>,
    #[arg(short = 'r', long = "revision")]
    pub revision: Option<String>,
    #[arg(long = "log-window-size")]
    pub log_window_size: Option<u32>,
    #[arg(long)]
    pub localtime: bool,
    #[arg(long = "no-metadata")]
    pub no_metadata: bool,
    #[arg(long = "rewrite-root")]
    pub rewrite_root: Option<String>,
    #[arg(long = "rewrite-uuid")]
    pub rewrite_uuid: Option<String>,
    #[arg(long)]
    pub username: Option<String>,
    #[arg(long = "config-dir")]
    pub config_dir: Option<String>,
    #[arg(long = "no-auth-cache")]
    pub no_auth_cache: bool,
    #[arg(long = "preserve-empty-dirs")]
    pub preserve_empty_dirs: bool,
    #[arg(long = "placeholder-filename", default_value = ".gitignore")]
    pub placeholder_filename: String,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    pub url: String,
    pub path: Option<String>,
    #[command(flatten)]
    pub layout: LayoutArgs,
    #[command(flatten)]
    pub shared: SharedFetchArgs,
}

#[derive(Debug, Args)]
pub struct CloneArgs {
    pub url: String,
    pub path: Option<String>,
    #[command(flatten)]
    pub layout: LayoutArgs,
    #[command(flatten)]
    pub shared: SharedFetchArgs,
    #[arg(long = "no-checkout")]
    pub no_checkout: bool,
}

#[derive(Debug, Args)]
pub struct FetchArgs {
    pub remote: Option<String>,
    #[command(flatten)]
    pub shared: SharedFetchArgs,
    #[arg(long = "fetch-all", alias = "all")]
    pub fetch_all: bool,
    #[arg(short = 'p', long = "parent")]
    pub parent: bool,
}

#[derive(Debug, Args)]
pub struct RebaseArgs {
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,
    #[arg(short = 'm', long = "merge")]
    pub merge: bool,
    #[arg(short = 's', long = "strategy")]
    pub strategy: Option<String>,
    #[command(flatten)]
    pub shared: SharedFetchArgs,
}

#[derive(Debug, Args)]
pub struct DcommitArgs {
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,
    #[arg(long = "commit-url")]
    pub commit_url: Option<String>,
    #[arg(long = "mergeinfo")]
    pub mergeinfo: Option<String>,
    #[arg(long = "no-rebase")]
    pub no_rebase: bool,
    #[command(flatten)]
    pub shared: SharedFetchArgs,
}

#[derive(Debug, Args)]
pub struct LogArgs {
    #[arg(short = 'r', long = "revision")]
    pub revision: Option<String>,
    #[arg(long)]
    pub limit: Option<u32>,
    #[arg(short = 'v', long)]
    pub verbose: bool,
    #[arg(long)]
    pub incremental: bool,
    #[arg(long)]
    pub oneline: bool,
    #[arg(long = "show-commit")]
    pub show_commit: bool,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub git_log_args: Vec<String>,
}

#[derive(Debug, Args)]
pub struct InfoArgs {
    #[arg(long)]
    pub url: bool,
}

#[derive(Debug, Args)]
pub struct FindRevArgs {
    pub rev_or_commit: String,
    #[arg(short = 'B', long = "before")]
    pub before: bool,
    #[arg(short = 'A', long = "after")]
    pub after: bool,
}

#[derive(Debug, Args)]
pub struct GcArgs {}

#[derive(Debug, Args)]
pub struct ResetArgs {
    #[arg(short = 'r', long = "revision")]
    pub revision: String,
    #[arg(short = 'p', long = "parent")]
    pub parent: bool,
}

#[derive(Debug, Args)]
pub struct DiagnoseArgs {}

#[derive(Debug, Args)]
pub struct UnsupportedArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}
