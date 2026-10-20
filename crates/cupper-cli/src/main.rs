//! `cupper`, the command-line client (design 5.2.14): the commands are in
//! [`cupper_cli::run`], their parsing in [`cupper_cli::args`].

use std::process::ExitCode;

use cupper_cli::args::{self, Command};
use cupper_cli::build_kind::{BUILD, build_info};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (global, command) = match args::parse(&args) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("cupper: {err}\n\n{}", args::USAGE);
            return ExitCode::from(2);
        }
    };
    if command == Command::BuildInfo {
        println!("{}", build_info(BUILD));
        return ExitCode::SUCCESS;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let Ok(runtime) = runtime else {
        return ExitCode::FAILURE;
    };
    ExitCode::from(runtime.block_on(cupper_cli::run::main(global, command)))
}
