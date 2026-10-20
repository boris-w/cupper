//! `cupper-host`, the session host process (design 5.2). It runs steps 1 and 2
//! of the startup sequence (design 5.2.13): the command line, `--build-info`,
//! in `dev-auth` builds the launcher (design 5.8.1), and the stdin payload;
//! then `host.log`, and stdio goes to `/dev/null`. The supervisor runs the other steps on the tokio
//! workers, while the main thread runs its `CFRunLoop` until the host exits
//! (design 5.2.3).

use std::process::ExitCode;
use std::sync::Arc;

use cupper_host::clock::SystemClock;
use cupper_host::diag;
use cupper_host::exit::HostExit;
use cupper_host::startup::{self, Command, payload};
use cupper_host::supervisor::{Host, Start};
use cupper_sys::{RunLoop, RunLoopStop};

fn main() -> ExitCode {
    let data_dir = match Command::parse(std::env::args_os().skip(1), std::env::var_os("HOME")) {
        Ok(Command::Run { data_dir }) => data_dir,
        Ok(Command::BuildInfo) => {
            println!("{}", startup::build_info());
            return HostExit::Ok.into();
        }
        Err(err) => return fail(&err, err.exit()),
    };
    #[cfg(feature = "dev-auth")]
    let launcher = match startup::launcher() {
        Ok(launcher) => launcher,
        Err(err) => return fail(&err, HostExit::Error),
    };
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get().min(4));
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => return fail(&err, HostExit::Error),
    };
    let clock = SystemClock::new();
    let load = payload::load(std::io::stdin(), &clock, payload::TIMEOUT);
    let payload = match runtime.block_on(load) {
        Ok(payload) => payload,
        Err(err) => return fail(&err, err.exit()),
    };
    #[cfg(feature = "test-harness")]
    if let Err(err) = cupper_host::failpoint::arm(&payload.test_config.failpoints) {
        return fail(&err, HostExit::Error);
    }
    // Step 2, while stderr can still tell when the log cannot be started.
    let logs = diag::log::log_dir(&data_dir);
    let echo = cfg!(all(feature = "dev-auth", not(feature = "test-harness")));
    if let Err(err) = diag::log::init(&logs, Arc::new(clock), echo) {
        eprintln!("cupper-host: no host.log in {}: {err}", logs.display());
    }
    if let Err(err) = cupper_sys::redirect_stdio_to_null() {
        return logged(&err, HostExit::Error);
    }
    let start = Start {
        data_dir,
        payload,
        #[cfg(feature = "dev-auth")]
        launcher,
    };
    // The code tells the desktop what to do (design 5.7.1); the reason goes
    // to the log.
    let host = match runtime.block_on(Host::start(start)) {
        Ok(host) => host,
        Err(err) => return logged(&err, err.exit()),
    };
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "the host started");
    let main_loop = RunLoop::current();
    let stop = StopOnDrop(main_loop.stopper());
    let running = runtime.spawn(async move {
        let _stop = stop;
        host.run().await
    });
    main_loop.run();
    let exit = runtime.block_on(running).unwrap_or(HostExit::Error);
    tracing::info!(code = exit.code(), "the host exits");
    // Nothing of the host is left to wait for.
    runtime.shutdown_background();
    exit.into()
}

/// Stops the main thread's run loop as the host exits, a panic included.
struct StopOnDrop(RunLoopStop);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.stop();
    }
}

/// Before the log: the reason goes to stderr, for the launcher to see.
fn fail(err: &dyn std::fmt::Display, exit: HostExit) -> ExitCode {
    eprintln!("cupper-host: {err}");
    exit.into()
}

/// After it: stdio is `/dev/null`, so the reason goes to `host.log`.
fn logged(err: &dyn std::fmt::Display, exit: HostExit) -> ExitCode {
    tracing::error!(code = exit.code(), "the host cannot run: {err}");
    exit.into()
}
