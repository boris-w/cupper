//! Minimal UDS server that authenticates each peer by audit token + code signature.
//!
//! peer-server --socket PATH [--socket PATH ...] [--require REQ] [--flags default|kernel|nores|...]
//!             [--no-runtime-check] [--no-gta-check] [--delay-ms N] [--recheck] [--pin] [--log FILE]
//!
//! --pin: verify the audit token captured at accept time, send {"challenge": nonce}, require the
//! first line to echo the nonce and every later message to arrive with the same token.
//!
//! Protocol: the client sends lines; the server answers each line with one JSON line.
//! Without --recheck only the first line is authenticated and later lines inherit the verdict.

use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use sp4b::*;

struct Cfg {
    policy: Policy,
    delay_ms: u64,
    recheck: bool,
    pin: bool,
    log: Option<Arc<Mutex<std::fs::File>>>,
}

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()
}

fn lineage_json(pid: i32) -> Value {
    let chain: Vec<Value> = ancestors(pid)
        .iter()
        .map(|p| json!({"pid": p.pid, "comm": p.comm, "pgid": p.pgid, "sid": p.sid}))
        .collect();
    json!({"chain": chain, "responsible_pid": responsible_pid(pid)})
}

fn emit(cfg: &Cfg, v: &Value) {
    let line = v.to_string();
    println!("{line}");
    if let Some(f) = &cfg.log {
        let mut f = f.lock().unwrap();
        let _ = writeln!(f, "{line}");
    }
}

fn handle(stream: UnixStream, sock: String, cfg: Arc<Cfg>) {
    let fd = stream.as_raw_fd();
    let at_accept = json!({
        "uid": peer_uid(fd).ok(),
        "pid": peer_pid(fd).ok(),
        "epid": peer_epid(fd).ok(),
        "token": peer_token(fd).ok().map(|t| token_json(&t)),
    });
    let accept_token = peer_token(fd).ok();
    let accept_pid = peer_pid(fd).ok();
    let lineage = accept_pid.map(lineage_json);
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = stream;
    let mut authed: Option<bool> = None;
    let mut first_token: Option<AuditToken> = None;
    let mut n = 0;
    // --pin: verify the token captured at accept, send a challenge, and require every
    // later message to carry the same token (same pid and pidversion).
    let mut nonce = String::new();
    let mut pinned: Option<(bool, Value)> = None;
    if cfg.pin {
        let v = match &accept_token {
            Some(t) => verify_token(t, &cfg.policy),
            None => Verdict { accept: false, stage: "peer_token", detail: json!({"error": "no token at accept"}) },
        };
        pinned = Some((v.accept, json!({"accept": v.accept, "stage": v.stage, "detail": v.detail})));
        let mut b = [0u8; 8];
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            use std::io::Read;
            let _ = f.read_exact(&mut b);
        }
        nonce = hex(&b);
        if writeln!(writer, "{}", json!({"challenge": nonce})).is_err() {
            return;
        }
    }
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        n += 1;
        if cfg.delay_ms > 0 && n == 1 {
            std::thread::sleep(Duration::from_millis(cfg.delay_ms));
        }
        let tok = peer_token(fd);
        let pid_now = peer_pid(fd).ok();
        let do_check = authed.is_none() || cfg.recheck || cfg.pin;
        let mut verdict_json = Value::Null;
        if let Some((ok0, vj)) = &pinned {
            let same = match (&accept_token, &tok) {
                (Some(a), Ok(b)) => a == b,
                _ => false,
            };
            let challenge_ok = n > 1 || line.trim_end() == nonce;
            let ok = *ok0 && same && challenge_ok && authed.unwrap_or(true);
            authed = Some(ok);
            let stage = if !ok0 {
                "pinned_token_rejected"
            } else if !same {
                "token_changed"
            } else if !challenge_ok {
                "bad_challenge"
            } else {
                "accepted"
            };
            verdict_json = json!({"accept": ok, "stage": stage, "pinned": vj, "token_same_as_accept": same});
        } else if do_check {
            let v = match &tok {
                Ok(t) => verify_token(t, &cfg.policy),
                Err(e) => Verdict { accept: false, stage: "peer_token", detail: json!({"error": e.to_string()}) },
            };
            // Re-check also requires the identity to stay the same as the first authenticated token.
            let same = match (&first_token, &tok) {
                (Some(a), Ok(b)) => a == b,
                _ => true,
            };
            let ok = v.accept && same;
            authed = Some(match authed {
                None => ok,
                Some(prev) => prev && ok,
            });
            if first_token.is_none() {
                first_token = tok.as_ref().ok().copied();
            }
            verdict_json = json!({"accept": v.accept, "stage": v.stage, "detail": v.detail, "token_same_as_first": same});
        }
        let accepted = authed.unwrap_or(false);
        let record = json!({
            "ts_ms": now_ms(),
            "socket": sock,
            "msg_index": n,
            "msg": line.trim_end(),
            "at_accept": if n == 1 { at_accept.clone() } else { Value::Null },
            "lineage": if n == 1 { lineage.clone().unwrap_or(Value::Null) } else { Value::Null },
            "accept_token_vs_now_equal": accept_token.as_ref().zip(tok.as_ref().ok()).map(|(a, b)| a == b),
            "peer_pid_now": pid_now,
            "token_now": tok.as_ref().ok().map(token_json),
            "checked": do_check,
            "verdict": verdict_json,
            "connection_accepted": accepted,
            "policy": if n == 1 { cfg.policy.json() } else { Value::Null },
        });
        emit(&cfg, &record);
        let reply = json!({"accept": accepted, "msg_index": n, "verdict_stage": record["verdict"]["stage"]});
        if writeln!(writer, "{reply}").is_err() {
            break;
        }
    }
}

fn main() {
    let a = Args::from_env();
    let sockets = a.opts("--socket");
    assert!(!sockets.is_empty(), "--socket PATH required");
    let policy = Policy {
        requirement: a.opt("--require"),
        check_flags: a.opt("--flags").map(|s| parse_flags(&s)).unwrap_or(0),
        require_runtime: !a.flag("--no-runtime-check"),
        forbid_get_task_allow: !a.flag("--no-gta-check"),
    };
    let log = a.opt("--log").map(|p| {
        Arc::new(Mutex::new(
            std::fs::OpenOptions::new().create(true).append(true).open(p).expect("open log"),
        ))
    });
    let cfg = Arc::new(Cfg {
        policy,
        delay_ms: a.opt("--delay-ms").map(|s| s.parse().unwrap()).unwrap_or(0),
        recheck: a.flag("--recheck"),
        pin: a.flag("--pin"),
        log,
    });
    let mut handles = vec![];
    for s in sockets {
        if let Some(dir) = std::path::Path::new(&s).parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let _ = std::fs::remove_file(&s);
        let l = UnixListener::bind(&s).unwrap_or_else(|e| panic!("bind {s}: {e}"));
        // World-accessible socket file on purpose: the experiments must show what
        // authentication alone achieves, independent of filesystem permissions.
        std::fs::set_permissions(&s, std::fs::Permissions::from_mode(0o777)).ok();
        eprintln!("listening on {s} (pid {})", std::process::id());
        let cfg = cfg.clone();
        handles.push(std::thread::spawn(move || {
            for c in l.incoming() {
                match c {
                    Ok(stream) => {
                        let cfg = cfg.clone();
                        let s2 = s.clone();
                        std::thread::spawn(move || handle(stream, s2, cfg));
                    }
                    Err(e) => eprintln!("accept: {e}"),
                }
            }
        }));
    }
    for h in handles {
        let _ = h.join();
    }
}
