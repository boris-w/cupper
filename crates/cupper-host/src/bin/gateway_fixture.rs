//! `cupper-gateway-fixture`: the gateway alone, for process-level tests
//! (IT-devkey-launcher-only, IT-devkey-orphan-start, T073's UNIT-native).
//! Only `test-harness` builds have it.
//!
//! It starts like the host (design 5.2.13), except that `--data-dir <dir>`
//! is required, so it never takes the user's own data directory; it is
//! created if missing. Then come, in `dev-auth` builds, the launcher check
//! and the startup payload on stdin, whose keys it accepts and whose
//! `test_config.durations` it runs on. Once listening, it prints
//! `{"socket": "<path>"}` on stdout, then writes each audit record to stderr
//! as a line of JSON: `{"conn": "c_1", "event": "<the event, as Rust's
//! Debug>"}`. It serves until killed and otherwise exits with the host's
//! codes.

use std::io::Write;
use std::os::unix::fs::DirBuilderExt;
use std::process::ExitCode;
use std::sync::Arc;

use cupper_host::clock::SystemClock;
use cupper_host::exit::HostExit;
use cupper_host::gateway::listen::HostLock;
use cupper_host::gateway::{Gateway, GatewayConfig};
use cupper_host::startup::{self, Command, payload};
use cupper_protocol::ids::HostId;

fn main() -> ExitCode {
    let data_dir = match Command::parse(std::env::args_os().skip(1), None) {
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
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => return fail(&err, HostExit::Error),
    };
    let clock = Arc::new(SystemClock::new());
    let load = payload::load(std::io::stdin(), &*clock, payload::TIMEOUT);
    let payload = match runtime.block_on(load) {
        Ok(payload) => payload,
        Err(err) => return fail(&err, err.exit()),
    };
    let dir = std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&data_dir);
    if let Err(err) = dir {
        return fail(&err, HostExit::DataDirUnusable);
    }
    let _runtime = runtime.enter();
    let bound = HostLock::acquire(&data_dir).and_then(|lock| Ok((lock.bind()?, lock)));
    let ((listener, socket), _lock) = match bound {
        Ok(bound) => bound,
        Err(err) => return fail(&err, err.exit()),
    };
    let config = GatewayConfig {
        host_id: HostId::from_uuid(startup::new_host_uuid().as_u128()),
        data_epoch: 1,
        ui_requirement: None,
        #[cfg(feature = "dev-auth")]
        dev_key: payload
            .dev_key
            .map(|key| cupper_host::gateway::DevKey { key, launcher }),
        harness_key: payload.harness_key,
    };
    let (gateway, mut audit) = Gateway::new(config, clock, payload.test_config.durations);
    println!("{}", serde_json::json!({"socket": socket}));
    runtime.block_on(async move {
        tokio::spawn(gateway.serve(listener));
        while let Some(record) = audit.recv().await {
            let event = format!("{:?}", record.event);
            let line = serde_json::json!({"conn": record.conn, "event": event});
            // Whether anyone reads stderr is the launcher's choice.
            let _ = writeln!(std::io::stderr(), "{line}");
        }
    });
    HostExit::Ok.into()
}

fn fail(err: &dyn std::fmt::Display, exit: HostExit) -> ExitCode {
    eprintln!("cupper-gateway-fixture: {err}");
    exit.into()
}
