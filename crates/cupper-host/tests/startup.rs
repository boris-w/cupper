//! The startup contract as the launcher sees it: the `cupper-host` binary,
//! its arguments, stdin and exit codes (design 5.2.13, 5.8.1).

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn host(args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cupper-host"));
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

fn run(cmd: &mut Command) -> Output {
    cmd.output().expect("run cupper-host")
}

/// Runs the host with `--data-dir` (which a bad payload keeps it from
/// touching) and `input` on stdin, then closes stdin.
#[cfg(any(feature = "dev-auth", feature = "test-harness"))]
fn run_with_stdin(input: &[u8]) -> (Option<i32>, String) {
    let data_dir = concat!(env!("CARGO_TARGET_TMPDIR"), "/cupper-host-startup");
    let mut child = host(&["--data-dir", data_dir])
        .stdin(Stdio::piped())
        .spawn()
        .expect("start cupper-host");
    let mut stdin = child.stdin.take().expect("a stdin pipe");
    // A host that does not read stdin may be gone already.
    let _ = stdin.write_all(input);
    drop(stdin);
    let out = child.wait_with_output().expect("wait for cupper-host");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn build_info() -> serde_json::Value {
    let out = run(&mut host(&["--build-info"]));
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    serde_json::from_slice(&out.stdout).expect("--build-info prints JSON")
}

#[test]
fn build_info_prints_the_features_of_the_binary() {
    assert_eq!(
        build_info()["features"],
        serde_json::json!(cupper_host::startup::FEATURES)
    );
}

/// IT-runtime-install: outside a bundle, none of the manifest's files is
/// there; the runtime directory is only read.
#[test]
fn it_runtime_install_self_test_outside_a_bundle_exits_1() {
    let data_dir = concat!(env!("CARGO_TARGET_TMPDIR"), "/cupper-host-self-test");
    let out = run(&mut host(&["--self-test", "--data-dir", data_dir]));
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["ok"], false);
    let problems = report["problems"].as_array().unwrap();
    assert_eq!(problems.len() as u64, report["files"].as_u64().unwrap());
    assert!(
        problems.iter().all(|p| p["problem"] == "missing"),
        "{report}"
    );
    let runtime = serde_json::json!({"esbuild-wasm": "missing", "node": "missing"});
    assert_eq!(report["runtime"], runtime);
    assert!(!std::path::Path::new(data_dir).exists());
}

#[test]
fn unknown_argument_exits_1() {
    let out = run(&mut host(&["--self-tests"]));
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage: cupper-host"));
}

/// The release pipeline's check (design 5.10.7): a build without features says
/// so in `--build-info`.
#[cfg(not(any(feature = "dev-auth", feature = "test-harness")))]
#[test]
fn unit_release_features_build_info_names_no_test_feature() {
    let out = run(&mut host(&["--build-info"]));
    let text = String::from_utf8_lossy(&out.stdout);
    for feature in ["dev-auth", "test-harness"] {
        assert!(!text.contains(feature), "{feature} in {text}");
    }
    assert_eq!(build_info()["features"], serde_json::json!([]));
}

/// Builds that read the payload exit with 1 on a bad one (design 5.8.1).
#[cfg(any(feature = "dev-auth", feature = "test-harness"))]
#[test]
fn unit_startup_payload_bad_payload_exits_1() {
    let cases: [(&[u8], &str); 3] = [
        (b"{not json\n", "malformed startup payload"),
        (
            b"{\"dev_key\": \"\", \"unknown\": 1}\n",
            "malformed startup payload",
        ),
        (b"", "stdin ended without a startup payload"),
    ];
    for (input, message) in cases {
        let (code, stderr) = run_with_stdin(input);
        assert_eq!(code, Some(1), "{input:?}: {stderr}");
        assert!(stderr.contains(message), "{input:?}: {stderr}");
    }
}

/// A release host never reads stdin: a payload that every reading build
/// rejects goes unnoticed, and the host comes up. Its stdio went to
/// `/dev/null` at once, so its stderr stays empty; SIGTERM ends it.
#[cfg(not(any(feature = "dev-auth", feature = "test-harness")))]
#[tokio::test]
async fn unit_release_features_host_does_not_read_stdin() {
    use std::time::Duration;

    use cupper_host::clock::{Clock, SystemClock};

    let payload = br#"{"harness_key": "not a key", "test_config": {"failpoints": [{"name": "no.such"}]}, "unknown": 1}"#;
    let data_dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("cupper-host-release-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let mut child = host(&["--data-dir", data_dir.to_str().unwrap()])
        .stdin(Stdio::piped())
        .spawn()
        .expect("start cupper-host");
    let _ = child.stdin.take().unwrap().write_all(payload);
    let socket =
        cupper_protocol::transport::socket_path(&data_dir, &cupper_sys::user_temp_dir().unwrap());
    let clock = SystemClock::new();
    let give_up = clock.now_mono() + Duration::from_secs(30);
    while tokio::net::UnixStream::connect(&socket).await.is_err() {
        assert!(clock.now_mono() < give_up, "the host does not listen");
        clock
            .timer_at(clock.now_mono() + Duration::from_millis(20))
            .await;
    }
    let kill = Command::new("/bin/kill")
        .args(["-TERM", &child.id().to_string()])
        .status();
    assert!(kill.unwrap().success());
    let out = child.wait_with_output().expect("wait for cupper-host");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(out.stderr.is_empty(), "{out:?}");
    // A data directory outside Application Support keeps its own logs (T028).
    let log = std::fs::read_to_string(data_dir.join("logs/host.log")).expect("host.log");
    assert!(
        log.contains("the host started") && log.contains("the host exits"),
        "{log}"
    );
    std::fs::remove_dir_all(&data_dir).unwrap();
}
