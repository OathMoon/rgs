use anyhow::{Result, bail};
use git_svn_rs_core::cli::{Cli, Command};
use git_svn_rs_core::commands;
use git_svn_rs_core::diagnostics;

fn main() -> Result<()> {
    let cli = Cli::parse_compat();
    if cli.quiet {
        bail!("global --quiet is not supported in v1");
    }
    if cli.verbose > 0 {
        bail!("global --verbose is not supported in v1");
    }
    match cli.command {
        Command::Init(args) => commands::init::run(args).map_err(anyhow::Error::msg),
        Command::Clone(args) => {
            let output = commands::clone::run_with_output(args).map_err(anyhow::Error::msg)?;
            print!("{}", output.stdout);
            eprint!("{}", output.stderr);
            Ok(())
        }
        Command::Fetch(args) => commands::fetch::run(args).map_err(anyhow::Error::msg),
        Command::Rebase(args) => {
            print!(
                "{}",
                commands::rebase::run_with_inherited_stderr(args).map_err(anyhow::Error::msg)?
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
            println!("git-svn-rs version: {}", diagnostics::package_version());
            println!(
                "frozen git-svn baseline: {} ({})",
                diagnostics::FROZEN_GIT_SVN_VERSION,
                diagnostics::FROZEN_GIT_COMMIT
            );
            println!("platform: {}", diagnostics::platform());
            println!("libsvn feature: {}", diagnostics::libsvn_feature_status());
            println!("libsvn link: {}", diagnostics::libsvn_link_status());
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
