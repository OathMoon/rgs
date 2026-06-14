use anyhow::{bail, Result};
use clap::Parser;
use git_svn_rs_core::cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Diagnose(_) => {
            println!("git-svn-rs diagnostics");
            println!("libsvn feature: disabled");
            Ok(())
        }
        Command::Unsupported(args) => {
            let name = args.first().cloned().unwrap_or_else(|| "unknown".to_string());
            bail!("unsupported in v1: {name}")
        }
        Command::Branch(_) => bail!("unsupported in v1: branch"),
        Command::Tag(_) => bail!("unsupported in v1: tag"),
        Command::SetTree(_) => bail!("unsupported in v1: set-tree"),
        Command::Propget(_) => bail!("unsupported in v1: propget"),
        Command::Propset(_) => bail!("unsupported in v1: propset"),
        Command::Proplist(_) => bail!("unsupported in v1: proplist"),
        Command::ShowIgnore(_) => bail!("unsupported in v1: show-ignore"),
        Command::ShowExternals(_) => bail!("unsupported in v1: show-externals"),
        other => bail!("command parsed but not implemented in phase 1: {other:?}"),
    }
}
