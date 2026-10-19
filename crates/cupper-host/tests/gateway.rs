//! The gateway as a process (design 5.8.1): `dev_key` is only accepted from
//! the process that started the host (IT-devkey-launcher-only), and a host
//! whose launcher has already gone exits (IT-devkey-orphan-start).

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, Command, ExitStatus, Stdio};
use std::time::Duration;

use cupper_host::gateway::testkit::{Client, KEY, TempDir, initialize_params, key_auth};
use cupper_protocol::frame::{self, FrameDecoder};
use cupper_protocol::proof::{self, Direction};
use cupper_sys::ExitWatch;
use serde_json::{Value, json};

const FIXTURE: &str = env!("CARGO_BIN_EXE_cupper-gateway-fixture");
const HOST: &str = env!("CARGO_BIN_EXE_cupper-host");
const GONE: &str = "the process that started the host has already exited";

/// The fixture, started by this process with [`KEY`] as `dev_key`.
struct Fixture {
    child: Child,
    socket: PathBuf,
    audit: BufReader<ChildStderr>,
    _dir: TempDir,
}

impl Fixture {
    fn start() -> Self {
        let dir = TempDir::new("fixture");
        let mut child = Command::new(FIXTURE)
            .arg("--data-dir")
            .arg(dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start the fixture");
        let payload = json!({"dev_key": proof::to_hex(&KEY)});
        writeln!(child.stdin.take().unwrap(), "{payload}").unwrap();
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        let ready: Value = serde_json::from_str(&ready).expect("the socket line");
        Self {
            socket: ready["socket"].as_str().unwrap().into(),
            audit: BufReader::new(child.stderr.take().unwrap()),
            child,
            _dir: dir,
        }
    }

    /// The next audit record of a connection that closed.
    fn closed(&mut self) -> String {
        loop {
            let mut line = String::new();
            assert!(self.audit.read_line(&mut line).unwrap() > 0, "audit ended");
            let record: Value = serde_json::from_str(&line).unwrap();
            let event = record["event"].as_str().unwrap();
            if event.starts_with("Closed") {
                return event.to_owned();
            }
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `initialize` of the desktop UI's control connection.
fn ui_params(nonce: &str, auth: Value) -> Value {
    let mut params = initialize_params(nonce, auth);
    params["client"]["kind"] = json!("desktop");
    params["role"] = json!("control");
    params
}

#[tokio::test]
async fn it_devkey_launcher_only_accepts_the_process_that_started_the_host() {
    let mut fixture = Fixture::start();
    let host_pid = fixture.child.id() as i32;
    let me = std::process::id() as i32;
    let mut ui = Client::connect(&fixture.socket).await;
    let nonce = ui.hello().await;
    let (auth, channel) = key_auth("dev_key", &KEY, &nonce, me);
    let result = ui.call("initialize", ui_params(&nonce, auth)).await;
    let result = result.expect("the launcher is accepted");
    assert_eq!(result["principal"]["id"], "pr_desktop");
    let host_proof = result["host_proof"].as_str().unwrap();
    let checked = channel.check(&KEY, Direction::HostToClient, host_pid, host_proof);
    assert_eq!(checked, Ok(()));

    // `nc` holds the same key, and its proof is right for its own pid.
    let mut nc = Command::new("/usr/bin/nc")
        .arg("-U")
        .arg(&fixture.socket)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start nc");
    let (mut to_nc, mut from_nc) = (nc.stdin.take().unwrap(), nc.stdout.take().unwrap());
    let mut decoder = FrameDecoder::new();
    let hello = next(&mut from_nc, &mut decoder);
    let nonce = hello["params"]["nonce"].as_str().unwrap();
    let (auth, _) = key_auth("dev_key", &KEY, nonce, nc.id() as i32);
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": ui_params(nonce, auth)});
    to_nc
        .write_all(&frame::encode_json(&request).unwrap())
        .unwrap();
    let error = &next(&mut from_nc, &mut decoder)["error"];
    assert_eq!(error["data"]["reason"], "bad_key", "{error}");
    drop(to_nc);
    assert!(nc.wait().is_ok());
    // Turned away by the launcher rule, with nothing wrong in the proof.
    let closed = fixture.closed();
    assert!(closed.contains("HandshakeFailed"), "{closed}");
    assert!(closed.contains("process that started the host"), "{closed}");
    assert_eq!(ui.call("ping", Value::Null).await, Ok(json!({})));
}

/// The next message `nc` passes on from the host.
fn next(from_nc: &mut impl Read, decoder: &mut FrameDecoder) -> Value {
    let mut buf = [0; 4096];
    loop {
        if let Some(frame) = decoder.next_frame().unwrap() {
            return serde_json::from_slice(&frame.payload).unwrap();
        }
        let read = from_nc.read(&mut buf).unwrap();
        assert!(read > 0, "nc ended");
        decoder.push(&buf[..read]);
    }
}

#[test]
fn it_devkey_orphan_start_exits_1() {
    for program in [FIXTURE, HOST] {
        let dir = TempDir::new("orphan");
        let (status, stderr) = start_orphaned(program, dir.path());
        assert_eq!(status.code(), Some(1), "{program}: {stderr}");
        assert!(stderr.contains(GONE), "{program}: {stderr}");
        assert!(!dir.path().join("host.lock").exists(), "{program}");
    }
}

/// Runs `program` once its parent has exited and launchd has adopted it;
/// its exit status, read through kqueue, and its stderr.
fn start_orphaned(program: &str, data_dir: &Path) -> (ExitStatus, String) {
    // sh starts a subshell that waits for our line, then execs the program;
    // sh itself exits at once.
    let script = r#"exec 3<&0; (read go <&3; exec "$0" --data-dir "$1" 3<&-) & echo $!"#;
    let mut sh = Command::new("/bin/sh")
        .args(["-c", script, program])
        .arg(data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut go = sh.stdin.take().unwrap();
    let mut line = String::new();
    BufReader::new(sh.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let pid: i32 = line.trim().parse().unwrap();
    assert!(sh.wait().unwrap().success());
    assert_eq!(cupper_sys::process_info(pid).unwrap().ppid, 1, "adopted");

    let watch = ExitWatch::new(pid).unwrap();
    go.write_all(b"go\n").unwrap();
    let status = watch.wait(Duration::from_secs(30)).unwrap();
    let mut stderr = String::new();
    sh.stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    (status.expect("the orphan exits"), stderr)
}
