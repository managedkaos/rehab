//! Thin entry point: parse the CLI, dispatch to the run or undo flow, and set
//! the process exit code on error.

use std::process::ExitCode;

use rehab::cli::{Cli, Command};
use rehab::run;

fn main() -> ExitCode {
    let cli = Cli::parse_args();

    let result = match &cli.command {
        Some(Command::Undo(args)) => run::undo(args),
        None => run::run(&cli.run),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
