use anyhow::{Result, bail};
use clap::Parser;
use git_svn_rs_core::cli::{Cli, Command};
use git_svn_rs_core::commands;

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init(args) => commands::init::run(args).map_err(anyhow::Error::msg),
        Command::Clone(args) => commands::clone::run(args).map_err(anyhow::Error::msg),
        Command::Fetch(args) => commands::fetch::run(args).map_err(anyhow::Error::msg),
        Command::Rebase(args) => {
            print!(
                "{}",
                commands::rebase::run(args).map_err(anyhow::Error::msg)?
            );
            Ok(())
        }
        Command::FindRev(args) => {
            print!(
                "{}",
                commands::find_rev::run(args).map_err(anyhow::Error::msg)?
            );
            Ok(())
        }
        Command::Info(args) => {
            print!("{}", commands::info::run(args).map_err(anyhow::Error::msg)?);
            Ok(())
        }
        Command::Log(args) => {
            print!("{}", commands::log::run(args).map_err(anyhow::Error::msg)?);
            Ok(())
        }
        Command::Dcommit(args) => {
            print!(
                "{}",
                commands::dcommit::run(args).map_err(anyhow::Error::msg)?
            );
            Ok(())
        }
        Command::Gc(args) => commands::gc::run(args).map_err(anyhow::Error::msg),
        Command::Reset(args) => commands::reset::run(args).map_err(anyhow::Error::msg),
        Command::Diagnose(_) => {
            println!("git-svn-rs diagnostics");
            println!("libsvn feature: disabled");
            Ok(())
        }
        Command::Unsupported(args) => {
            let name = args
                .first()
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
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
    }
}
