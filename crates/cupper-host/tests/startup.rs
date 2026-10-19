//! The startup contract as the launcher sees it: the `cupper-host` binary,
//! its arguments, stdin and exit codes (design 5.2.13, 5.8.1).

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
