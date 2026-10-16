//! Test client. Optionally verifies the server (UI -> host direction) before talking.
//!
//! peer-client --socket PATH [--name NAME] [--server-require REQ] [--server-flags F]
//!             [--fd N]                       use an inherited, already connected fd
//!             [--lines K]                    send K lines (default 1)
//!             [--write-before-exec] [--exec -- PROG ARGS...]
//!                                            connect, optionally write, then exec PROG with the
//!                                            connection kept open as fd 9
//!             [--fork-exec [--parent-wait-ms N] -- PROG ARGS...]
//!                                            connect and fork; the child writes one line and execs
//!                                            PROG (fd 9 inherited); the parent keeps talking
//!             [--no-wait]                    write one line and exit without reading the reply
//!             [--idle-ms N]                  do nothing but sleep (stand-in for a genuine binary)
//!             [--sleep-before-ms N]          sleep after connect, before the first write
//!             [--challenge]                  answer the server's --pin challenge first
//!
//! Exit code: 0 accepted, 2 rejected by server, 3 server failed verification, 1 error.

use std::io::{BufRead, BufReader, Write};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::time::Duration;

use serde_json::json;
use sp4b::*;

/// --challenge: read the server's challenge line, echo the nonce and read the verdict.
fn answer_challenge(stream: &UnixStream, name: &str) {
    let mut r = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    r.read_line(&mut line).ok();
    let nonce = serde_json::from_str::<serde_json::Value>(&line)
        .ok()
        .and_then(|v| v["challenge"].as_str().map(String::from))
        .unwrap_or_default();
    let mut w = stream.try_clone().unwrap();
    writeln!(w, "{nonce}").ok();
    let mut reply = String::new();
    r.read_line(&mut reply).ok();
    println!("{}", json!({"client": name, "tag": "challenge", "pid": std::process::id(), "reply": reply.trim_end()}));
}

fn send_lines(stream: &UnixStream, name: &str, k: usize, tag: &str) -> bool {
    let mut w = stream.try_clone().unwrap();
    let mut r = BufReader::new(stream.try_clone().unwrap());
    let mut last = false;
    for i in 1..=k {
        if writeln!(w, "hello from {name} {tag} #{i}").is_err() {
            eprintln!("write failed");
            return false;
        }
        let mut line = String::new();
        if r.read_line(&mut line).unwrap_or(0) == 0 {
            eprintln!("no reply");
            return false;
        }
        println!("{}", json!({"client": name, "tag": tag, "i": i, "pid": std::process::id(), "reply": line.trim_end()}));
        last = line.contains("\"accept\":true");
    }
    last
}

fn clear_cloexec(fd: i32) {
    unsafe {
        let fl = libc::fcntl(fd, libc::F_GETFD);
        libc::fcntl(fd, libc::F_SETFD, fl & !libc::FD_CLOEXEC);
    }
}

fn main() {
    let a = Args::from_env();
    if let Some(ms) = a.opt("--idle-ms") {
        std::thread::sleep(Duration::from_millis(ms.parse().unwrap()));
        return;
    }
    let name = a.opt("--name").unwrap_or_else(|| "client".into());
    let lines: usize = a.opt("--lines").map(|s| s.parse().unwrap()).unwrap_or(1);
    let stream = match a.opt("--fd") {
        Some(fd) => unsafe { UnixStream::from_raw_fd(fd.parse().unwrap()) },
        None => {
            let path = a.opt("--socket").expect("--socket required");
            match UnixStream::connect(&path) {
                Ok(s) => s,
                Err(e) => {
                    println!("{}", json!({"client": name, "connect_error": e.to_string()}));
                    std::process::exit(1);
                }
            }
        }
    };
    if a.flag("--challenge") {
        answer_challenge(&stream, &name);
    }
    if let Some(ms) = a.opt("--sleep-before-ms") {
        std::thread::sleep(Duration::from_millis(ms.parse().unwrap()));
    }

    if let Some(req) = a.opt("--server-require") {
        let policy = Policy {
            requirement: Some(req),
            check_flags: a.opt("--server-flags").map(|s| parse_flags(&s)).unwrap_or(0),
            require_runtime: !a.flag("--no-runtime-check"),
            forbid_get_task_allow: true,
        };
        let fd = stream.as_raw_fd();
        let v = match peer_token(fd) {
            Ok(t) => verify_token(&t, &policy),
            Err(e) => Verdict { accept: false, stage: "peer_token", detail: json!({"error": e.to_string()}) },
        };
        println!(
            "{}",
            json!({"client": name, "server_check": {"accept": v.accept, "stage": v.stage, "detail": v.detail, "server_pid": peer_pid(fd).ok(), "policy": policy.json()}})
        );
        if !v.accept {
            std::process::exit(3);
        }
    }

    if a.flag("--exec") {
        let rest = a.rest();
        assert!(!rest.is_empty(), "missing PROG after --");
        if a.flag("--write-before-exec") {
            let mut w = stream.try_clone().unwrap();
            writeln!(w, "hello from {name} pre-exec").unwrap();
        }
        let fd = stream.into_raw_fd();
        unsafe { libc::dup2(fd, 9) };
        clear_cloexec(9);
        let err = std::process::Command::new(&rest[0]).args(&rest[1..]).exec();
        eprintln!("exec failed: {err}");
        std::process::exit(1);
    }

    if a.flag("--fork-exec") {
        // The child writes the first line and then execs PROG (fd 9 inherited);
        // the parent keeps the same connection and talks afterwards.
        let rest = a.rest();
        assert!(!rest.is_empty(), "missing PROG after --");
        let fd = stream.into_raw_fd();
        unsafe { libc::dup2(fd, 9) };
        clear_cloexec(9);
        let pid = unsafe { libc::fork() };
        if pid == 0 {
            let mut w = unsafe { UnixStream::from_raw_fd(9) };
            writeln!(w, "hello from {name} child pre-exec").unwrap();
            let _ = w.into_raw_fd();
            let err = std::process::Command::new(&rest[0]).args(&rest[1..]).exec();
            eprintln!("exec failed: {err}");
            std::process::exit(1);
        }
        std::thread::sleep(Duration::from_millis(
            a.opt("--parent-wait-ms").map(|s| s.parse().unwrap()).unwrap_or(800),
        ));
        let s = unsafe { UnixStream::from_raw_fd(9) };
        let mut r = BufReader::new(s.try_clone().unwrap());
        let mut line = String::new();
        r.read_line(&mut line).ok();
        println!("{}", json!({"client": name, "tag": "reply-to-child-line", "reply": line.trim_end()}));
        let ok = send_lines(&s, &name, lines, "parent-after-fork");
        let mut st = 0;
        unsafe { libc::waitpid(pid, &mut st, 0) };
        std::process::exit(if ok { 0 } else { 2 });
    }

    if a.flag("--no-wait") {
        let mut w = stream.try_clone().unwrap();
        writeln!(w, "hello from {name} (exiting without waiting)").unwrap();
        std::process::exit(0);
    }

    let ok = send_lines(&stream, &name, lines, "");
    if let Some(ms) = a.opt("--hold-ms") {
        std::thread::sleep(Duration::from_millis(ms.parse().unwrap()));
    }
    std::process::exit(if ok { 0 } else { 2 });
}
