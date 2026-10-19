//! UNIT-sys-spawn: `redirect_stdio_to_null` (design 5.8.1, 5.2.13). The call
//! changes the stdio of a whole process, so the test runs it in a child: this
//! test binary again, started with `CHILD` set.

use std::io::{Read, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::process::{Command, Stdio};

const CHILD: &str = "SYS_TEST_STDIO_NULL_CHILD";
const NAME: &str = "unit_sys_spawn_redirect_stdio_to_null";

/// In the child: redirect, then check fds 0–2 against `/dev/null` by rdev,
/// read stdin and write to stdout and stderr. Exits 0 when all is well.
fn child() -> ! {
    cupper_sys::redirect_stdio_to_null().unwrap();
    let null = std::fs::metadata("/dev/null").unwrap();
    let all_null = (0..=2).all(|fd| {
        let meta = std::fs::metadata(format!("/dev/fd/{fd}")).unwrap();
        meta.file_type().is_char_device() && meta.rdev() == null.rdev()
    });
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    println!("after the redirect");
    eprintln!("after the redirect");
    std::process::exit(if all_null && input.is_empty() { 0 } else { 1 });
}

#[test]
fn unit_sys_spawn_redirect_stdio_to_null() {
    if std::env::var_os(CHILD).is_some() {
        child();
    }
    let mut run = Command::new(std::env::current_exe().unwrap())
        .args([NAME, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The child may already have closed its end of the pipe.
    let _ = run.stdin.take().unwrap().write_all(b"not for the child\n");
    let out = run.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert!(!stdout.contains("after the redirect"), "{stdout}");
    assert!(!stderr.contains("after the redirect"), "{stderr}");
}
