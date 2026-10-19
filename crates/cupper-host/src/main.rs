//! `cupper-host`, the session host process (design 5.2). It runs step 1 of the
//! startup sequence (design 5.2.13): the command line, `--build-info` and the
//! stdin payload. The steps after it are not assembled yet, so the host then
//! exits with 1.

use std::process::ExitCode;

use cupper_host::clock::SystemClock;
use cupper_host::exit::HostExit;
use cupper_host::startup::payload::{self, Payload};
use cupper_host::startup::{self, Command};

fn main() -> ExitCode {
    let command = match Command::parse(std::env::args_os().skip(1), std::env::var_os("HOME")) {
        Ok(command) => command,
        Err(err) => return fail(&err, err.exit()),
    };
    if command == Command::BuildInfo {
        println!("{}", startup::build_info());
        return HostExit::Ok.into();
    }
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => return fail(&err, HostExit::Error),
    };
    let clock = SystemClock::new();
    let payload = match runtime.block_on(payload::load(std::io::stdin(), &clock, payload::TIMEOUT))
    {
        Ok(payload) => payload,
        Err(err) => return fail(&err, err.exit()),
    };
    #[cfg(feature = "test-harness")]
    if let Err(err) = cupper_host::failpoint::arm(&payload.test_config.failpoints) {
        return fail(&err, HostExit::Error);
    }
    not_assembled(payload)
}

/// Where the rest of the startup sequence takes over.
fn not_assembled(_payload: Payload) -> ExitCode {
    eprintln!("cupper-host: the host is not assembled yet");
    HostExit::Error.into()
}

fn fail(err: &dyn std::fmt::Display, exit: HostExit) -> ExitCode {
    eprintln!("cupper-host: {err}");
    exit.into()
}
