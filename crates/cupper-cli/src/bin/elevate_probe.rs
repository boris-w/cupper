//! `cupper-elevate-probe` (test builds only): asks a host for `write` as the
//! `cli` principal, the way `cupper elevate` does, and writes each answer to
//! a file as a JSON line (IT-elevate, design 5.8.4).
//!
//!     cupper-elevate-probe --data-dir <dir> --out <file> [--setsid]
//!
//! With `--setsid` it first makes a session of its own, as Claude's tool
//! commands are. Once granted, it calls `session/interrupt` of a session that
//! does not exist: `not_found` shows the scope was there, `forbidden` that it
//! was not. Then it exits, which ends the grant.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use cupper_cli::client::{self, Client, ClientError, Options};
use cupper_cli::verify::HostCheck;
use cupper_protocol::ids::SessionId;
use cupper_protocol::methods::admin::{AuthElevate, AuthElevateParams};
use cupper_protocol::methods::session::{SessionInterrupt, SessionInterruptParams};
use cupper_protocol::rpc::Scope;
use cupper_protocol::transport::socket_path;
use serde_json::{Value, json};

fn main() -> ExitCode {
    let (mut data_dir, mut out, mut setsid) = (None, None, false);
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--data-dir") => data_dir = args.next().map(PathBuf::from),
            Some("--out") => out = args.next().map(PathBuf::from),
            Some("--setsid") => setsid = true,
            _ => data_dir = None,
        }
    }
    let (Some(data_dir), Some(out)) = (data_dir, out) else {
        eprintln!("usage: cupper-elevate-probe --data-dir <dir> --out <file> [--setsid]");
        return ExitCode::from(2);
    };
    if setsid && let Err(err) = cupper_sys::setsid() {
        eprintln!("setsid: {err}");
        return ExitCode::FAILURE;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let Ok(runtime) = runtime else {
        return ExitCode::FAILURE;
    };
    let record = |step: &str, answer: Value| {
        let line = json!({ step: answer }).to_string() + "\n";
        let file = OpenOptions::new().create(true).append(true).open(&out);
        file.and_then(|mut file| file.write_all(line.as_bytes()))
            .is_ok()
    };
    runtime.block_on(async {
        let temp = cupper_sys::user_temp_dir().unwrap_or_default();
        let socket = socket_path(&data_dir, &temp);
        let client = match Client::connect(&socket, &HostCheck::Harness, Options::default()).await {
            Ok(client) => client,
            Err(err) => return record("connect", answer(Err(&err))),
        };
        let scopes = vec![Scope::Write];
        let reason = Some("IT-elevate".to_owned());
        let elevated = client
            .request::<AuthElevate>(AuthElevateParams { scopes, reason })
            .await;
        let granted = elevated
            .as_ref()
            .is_ok_and(|result| !result.granted.is_empty());
        if !record("elevate", answer(elevated.as_ref().map(|r| json!(r)))) || !granted {
            return granted;
        }
        let (command_id, session_id) = (client::command_id(), SessionId::from_uuid(u128::MAX));
        let expected_turn_id = None;
        let params = SessionInterruptParams {
            command_id,
            session_id,
            expected_turn_id,
        };
        let interrupted = client.request::<SessionInterrupt>(params).await;
        record("write", answer(interrupted.as_ref().map(|r| json!(r))))
    });
    ExitCode::SUCCESS
}

/// `{"ok": result}`, or `{"error": the RPC error}`.
fn answer(result: Result<Value, &ClientError>) -> Value {
    match result {
        Ok(result) => json!({ "ok": result }),
        Err(ClientError::Rpc(err)) => json!({ "error": err }),
        Err(err) => json!({ "error": { "message": err.to_string() } }),
    }
}
