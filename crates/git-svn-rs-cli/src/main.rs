use std::process::ExitCode;

use git_svn_rs_core::cli::{Cli, Command};
use git_svn_rs_core::commands;
use git_svn_rs_core::diagnostics;
use git_svn_rs_core::error::GitSvnError;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), GitSvnError> {
    let cli = Cli::parse_compat();
    if cli.quiet {
        return Err(GitSvnError::invalid_invocation(
            "global --quiet is not supported in v1",
        ));
    }
    if cli.verbose > 0 {
        return Err(GitSvnError::invalid_invocation(
            "global --verbose is not supported in v1",
        ));
    }
    match cli.command {
        Command::Init(args) => commands::init::run(args).map_err(GitSvnError::from_command_error),
        Command::Clone(args) => {
            let output =
                commands::clone::run_with_output(args).map_err(GitSvnError::from_command_error)?;
            print!("{}", output.stdout);
            eprint!("{}", output.stderr);
            Ok(())
        }
        Command::Fetch(args) => commands::fetch::run(args).map_err(GitSvnError::from_command_error),
        Command::Rebase(args) => {
            print!(
                "{}",
                commands::rebase::run_with_inherited_stderr(args)
                    .map_err(GitSvnError::from_command_error)?
            );
            Ok(())
        }
        Command::FindRev(args) => {
            print!(
                "{}",
                commands::find_rev::run(args).map_err(GitSvnError::from_command_error)?
            );
            Ok(())
        }
        Command::Info(args) => {
            print!(
                "{}",
                commands::info::run(args).map_err(GitSvnError::from_command_error)?
            );
            Ok(())
        }
        Command::Log(args) => {
            print!(
                "{}",
                commands::log::run(args).map_err(GitSvnError::from_command_error)?
            );
            Ok(())
        }
        Command::Dcommit(args) => {
            print!("{}", commands::dcommit::run_typed(args)?);
            Ok(())
        }
        Command::Gc(args) => commands::gc::run(args).map_err(GitSvnError::from_command_error),
        Command::Reset(args) => {
            print!(
                "{}",
                commands::reset::run(args).map_err(GitSvnError::from_command_error)?
            );
            Ok(())
        }
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
            Err(GitSvnError::unsupported_command(name))
        }
        Command::Branch(_) => Err(GitSvnError::unsupported_command("branch")),
        Command::Tag(_) => Err(GitSvnError::unsupported_command("tag")),
        Command::SetTree(_) => Err(GitSvnError::unsupported_command("set-tree")),
        Command::Propget(_) => Err(GitSvnError::unsupported_command("propget")),
        Command::Propset(_) => Err(GitSvnError::unsupported_command("propset")),
        Command::Proplist(_) => Err(GitSvnError::unsupported_command("proplist")),
        Command::ShowIgnore(_) => Err(GitSvnError::unsupported_command("show-ignore")),
        Command::ShowExternals(_) => Err(GitSvnError::unsupported_command("show-externals")),
    }
}
