//! `cupper-host`, the session host process (design 5.2). It runs step 1 of the
//! startup sequence (design 5.2.13): the command line, `--build-info`, in
//! `dev-auth` builds the launcher (design 5.8.1), and the stdin payload; then
//! stdio goes to `/dev/null`. The supervisor runs the other steps on the tokio
//! workers, while the main thread runs its `CFRunLoop` until the host exits
//! (design 5.2.3).

use std::process::ExitCode;

use cupper_host::clock::SystemClock;
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
    // A release build reads no payload, so it lets go of stdio at once.
    #[cfg(not(any(feature = "dev-auth", feature = "test-harness")))]
    if let Err(err) = cupper_sys::redirect_stdio_to_null() {
        return fail(&err, HostExit::Error);
    }
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
    #[cfg(any(feature = "dev-auth", feature = "test-harness"))]
    if let Err(err) = cupper_sys::redirect_stdio_to_null() {
        return fail(&err, HostExit::Error);
    }
    let start = Start {
        data_dir,
        payload,
        #[cfg(feature = "dev-auth")]
        launcher,
    };
    // From here on the reasons go to the log (T028); the code tells the
    // desktop what to do (design 5.7.1).
    let host = match runtime.block_on(Host::start(start)) {
        Ok(host) => host,
        Err(err) => return fail(&err, err.exit()),
    };
    let main_loop = RunLoop::current();
    let stop = StopOnDrop(main_loop.stopper());
    let running = runtime.spawn(async move {
        let _stop = stop;
        host.run().await
    });
    main_loop.run();
    let exit = runtime.block_on(running).unwrap_or(HostExit::Error);
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

fn fail(err: &dyn std::fmt::Display, exit: HostExit) -> ExitCode {
    eprintln!("cupper-host: {err}");
    exit.into()
}
