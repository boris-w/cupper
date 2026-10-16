//! Stand-in for CupperHost.app in the upgrade drill (item 3).
//!
//! bundle-host --socket PATH --bundle HELPER.app --runtime TOOL
//!
//! At start-up it records: its own version (from the bundle's Info.plist), the cdhash of its
//! running code, and the on-disk cdhash of TOOL. Then it answers one command per line:
//!   ping       version captured at start-up
//!   selfcheck  SecCodeCheckValidity(self) and running cdhash vs on-disk cdhash of the bundle
//!   spawn      compare TOOL's on-disk cdhash with the recorded one; spawn it only if equal
//!   readres    read Contents/Resources/version.txt of the bundle path *now*

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::Command;

use serde_json::{Value, json};
use sp4b::*;

fn static_cdhash(path: &str) -> Result<String, String> {
    let c = static_code_for_path(path).map_err(|s| format!("{s} {}", status_message(s)))?;
    let info = signing_info(&c, false).map_err(|s| format!("{s} {}", status_message(s)))?;
    info.cdhash.ok_or_else(|| "unsigned".into())
}

fn running_cdhash() -> String {
    let mut cd = [0u8; 20];
    let rc = unsafe {
        ffi::csops(std::process::id() as i32, ffi::CS_OPS_CDHASH, cd.as_mut_ptr() as *mut _, 20)
    };
    if rc == 0 { hex(&cd) } else { format!("csops error {}", std::io::Error::last_os_error()) }
}

fn plist_version(bundle: &str) -> String {
    let out = Command::new("/usr/bin/defaults")
        .args(["read", &format!("{bundle}/Contents/Info"), "CFBundleShortVersionString"])
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(e) => format!("error {e}"),
    }
}

fn main() {
    let a = Args::from_env();
    let sock = a.opt("--socket").expect("--socket");
    let bundle = a.opt("--bundle").expect("--bundle");
    let runtime = a.opt("--runtime").expect("--runtime");
    let version = plist_version(&bundle);
    let my_cdhash = running_cdhash();
    let runtime_cdhash = static_cdhash(&runtime);
    eprintln!("bundle-host v{version} pid {} cdhash {my_cdhash}", std::process::id());

    let _ = std::fs::remove_file(&sock);
    let l = UnixListener::bind(&sock).expect("bind");
    for c in l.incoming() {
        let Ok(stream) = c else { continue };
        let mut r = BufReader::new(stream.try_clone().unwrap());
        let mut w = stream;
        let mut line = String::new();
        while r.read_line(&mut line).unwrap_or(0) > 0 {
            let cmd = line.trim().to_string();
            line.clear();
            let reply: Value = match cmd.as_str() {
                "ping" => json!({"version_at_start": version, "pid": std::process::id()}),
                "selfcheck" => {
                    let check = code_self().map_err(|s| CheckError { status: s, message: status_message(s) })
                        .and_then(|c| check_code(&c, 0, None));
                    let disk = static_cdhash(&bundle);
                    let now = running_cdhash();
                    json!({
                        "check_self": match &check { Ok(()) => json!("ok"), Err(e) => e.json() },
                        "running_cdhash": now,
                        "bundle_replaced": disk.as_ref().map(|d| *d != now).unwrap_or(true),
                        "on_disk_cdhash": disk.unwrap_or_else(|e| format!("error: {e}")),
                    })
                }
                "spawn" => {
                    let now = static_cdhash(&runtime);
                    let same = matches!((&runtime_cdhash, &now), (Ok(a), Ok(b)) if a == b);
                    if same {
                        let out = Command::new(&runtime).arg("--idle-ms").arg("1").status();
                        json!({"spawned": true, "status": format!("{out:?}")})
                    } else {
                        json!({"spawned": false, "state": "pending_upgrade",
                               "recorded": runtime_cdhash.clone().unwrap_or_else(|e| e),
                               "on_disk": now.unwrap_or_else(|e| e)})
                    }
                }
                "readres" => {
                    let p = format!("{bundle}/Contents/Resources/version.txt");
                    json!({"version_txt_now": std::fs::read_to_string(&p).map(|s| s.trim().to_string()).unwrap_or_else(|e| format!("error: {e}"))})
                }
                other => json!({"error": format!("unknown command {other}")}),
            };
            if writeln!(w, "{}", json!({"cmd": cmd, "reply": reply})).is_err() {
                break;
            }
        }
    }
}
