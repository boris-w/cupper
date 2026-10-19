//! `cupper-host`, the session host process (design 5.2). Its startup contract
//! (arguments, the stdin payload and exit codes; design 5.2.13 and 5.8.1) is not
//! implemented yet, so the binary refuses to run.

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("cupper-host: the startup contract is not implemented yet");
    ExitCode::FAILURE
}
