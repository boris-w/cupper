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

/// Runs the host with `--data-dir` (which it does not touch yet) and `input`
/// on stdin, then closes stdin.
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

#[test]
fn unknown_argument_exits_1() {
    let out = run(&mut host(&["--self-test"]));
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
/// rejects goes unnoticed.
#[cfg(not(any(feature = "dev-auth", feature = "test-harness")))]
#[test]
fn unit_release_features_host_does_not_read_stdin() {
    let payload = br#"{"harness_key": "not a key", "test_config": {"failpoints": [{"name": "no.such"}]}, "unknown": 1}"#;
    let (code, stderr) = run_with_stdin(payload);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("not assembled"), "{stderr}");
    assert!(!stderr.contains("payload"), "{stderr}");
}
