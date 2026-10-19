//! `cupper-host`, the session host process (design 5.2). It runs step 1 of the
//! startup sequence (design 5.2.13): the command line and `--build-info`. The
//! steps after it are not assembled yet, so the host then exits with 1.

use std::process::ExitCode;

use cupper_host::exit::HostExit;
use cupper_host::startup::{self, Command};

fn main() -> ExitCode {
    let command = match Command::parse(std::env::args_os().skip(1), std::env::var_os("HOME")) {
        Ok(command) => command,
        Err(err) => {
            eprintln!("cupper-host: {err}");
            return err.exit().into();
        }
    };
    match command {
        Command::BuildInfo => {
            println!("{}", startup::build_info());
            HostExit::Ok.into()
        }
        Command::Run { .. } => {
            eprintln!("cupper-host: the host is not assembled yet");
            HostExit::Error.into()
        }
    }
}
