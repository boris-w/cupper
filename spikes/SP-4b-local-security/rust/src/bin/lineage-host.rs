//! Item 5: can the host tell that a connecting peer descends from an adapter process?
//!
//! lineage-host --socket PATH [--disclaim] [--log FILE] -- ADAPTER_PROG ARGS...
//!
//! Spawns ADAPTER_PROG with posix_spawn + POSIX_SPAWN_SETSID (own session and process group),
//! optionally with responsibility disclaimed (the attribute macOS uses to attribute a child's
//! TCC requests to itself instead of its parent). Records the adapter's pid, start time, pgid
//! and sid, then scores every connecting peer against four lineage signals. Exits when the
//! adapter exits and connections have been drained.

use std::ffi::CString;
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use sp4b::*;

const POSIX_SPAWN_SETSID: libc::c_short = 0x0400;

fn spawn_adapter(argv: &[String], disclaim: bool) -> (i32, bool) {
    let cargs: Vec<CString> = argv.iter().map(|a| CString::new(a.as_str()).unwrap()).collect();
    let mut ptrs: Vec<*mut libc::c_char> = cargs.iter().map(|c| c.as_ptr() as *mut _).collect();
    ptrs.push(std::ptr::null_mut());
    let mut attr: libc::posix_spawnattr_t = std::ptr::null_mut();
    let mut disclaimed = false;
    unsafe {
        libc::posix_spawnattr_init(&mut attr);
        libc::posix_spawnattr_setflags(&mut attr, POSIX_SPAWN_SETSID);
        if disclaim {
            type F = unsafe extern "C" fn(*mut libc::posix_spawnattr_t, libc::c_int) -> libc::c_int;
            let name = CString::new("responsibility_spawnattrs_setdisclaim").unwrap();
            let sym = libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr());
            if !sym.is_null() {
                let f: F = std::mem::transmute(sym);
                disclaimed = f(&mut attr, 1) == 0;
            }
        }
        let mut pid: libc::pid_t = 0;
        unsafe extern "C" {
            static environ: *const *mut libc::c_char;
        }
        let rc = libc::posix_spawn(&mut pid, cargs[0].as_ptr(), std::ptr::null(), &attr, ptrs.as_ptr(), environ);
        libc::posix_spawnattr_destroy(&mut attr);
        assert_eq!(rc, 0, "posix_spawn failed: {rc}");
        (pid, disclaimed)
    }
}

fn main() {
    let a = Args::from_env();
    let sock = a.opt("--socket").expect("--socket");
    let disclaim = a.flag("--disclaim");
    let log = a.opt("--log").map(|p| {
        Arc::new(Mutex::new(std::fs::OpenOptions::new().create(true).append(true).open(p).unwrap()))
    });
    let _ = std::fs::remove_file(&sock);
    let l = UnixListener::bind(&sock).expect("bind");
    l.set_nonblocking(true).unwrap();

    let (apid, disclaimed) = spawn_adapter(&a.rest(), disclaim);
    std::thread::sleep(Duration::from_millis(50));
    let ainfo = proc_info(apid).expect("adapter info");
    let adapter = json!({"pid": apid, "pgid": ainfo.pgid, "sid": ainfo.sid,
        "responsible_pid": responsible_pid(apid), "disclaim_requested": disclaim, "disclaimed": disclaimed});
    eprintln!("adapter {adapter}");
    if let Some(f) = &log {
        writeln!(f.lock().unwrap(), "{}", json!({"adapter": adapter})).ok();
    }

    let mut adapter_done_at: Option<std::time::Instant> = None;
    loop {
        match l.accept() {
            Ok((stream, _)) => {
                let fd = stream.as_raw_fd();
                let pid = peer_pid(fd).unwrap_or(-1);
                let chain = ancestors(pid);
                let me = chain.first().cloned();
                let in_chain = chain.iter().skip(1).any(|p| p.pid == apid && p.start == ainfo.start);
                let same_pgid = me.as_ref().map(|p| p.pgid == ainfo.pgid).unwrap_or(false);
                let same_sid = me.as_ref().map(|p| p.sid == ainfo.sid).unwrap_or(false);
                let rp = responsible_pid(pid);
                let same_resp = rp.is_some() && rp == responsible_pid(apid).or(Some(apid));
                stream.set_nonblocking(false).ok();
                stream.set_read_timeout(Some(Duration::from_secs(3))).ok();
                let mut line = String::new();
                BufReader::new(stream.try_clone().unwrap()).read_line(&mut line).ok();
                let rec = json!({
                    "label": line.trim(),
                    "peer_pid": pid,
                    "chain": chain.iter().map(|p| json!({"pid": p.pid, "comm": p.comm})).collect::<Vec<_>>(),
                    "signals": {
                        "ancestor_chain": in_chain,
                        "same_pgid": same_pgid,
                        "same_sid": same_sid,
                        "same_responsible_pid": same_resp,
                    },
                    "peer_pgid": me.as_ref().map(|p| p.pgid),
                    "peer_sid": me.as_ref().map(|p| p.sid),
                    "peer_responsible_pid": rp,
                    "detected_by_any": in_chain || same_pgid || same_sid || same_resp,
                });
                println!("{rec}");
                if let Some(f) = &log {
                    writeln!(f.lock().unwrap(), "{rec}").ok();
                }
                let mut w = stream;
                writeln!(w, "ok").ok();
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                let mut st = 0;
                if adapter_done_at.is_none() && unsafe { libc::waitpid(apid, &mut st, libc::WNOHANG) } == apid {
                    adapter_done_at = Some(std::time::Instant::now());
                }
                // Escaped children connect up to ~2 s after the adapter exits.
                if adapter_done_at.map(|t| t.elapsed() > Duration::from_secs(4)).unwrap_or(false) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => panic!("accept: {e}"),
        }
    }
}
