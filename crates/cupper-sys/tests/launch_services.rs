//! Registering an app bundle with LaunchServices for real (design 5.2.9,
//! rule 1). It changes the machine's LaunchServices database, so it runs on a
//! GitHub runner only (`GITHUB_ACTIONS`): in the `rust` job of every PR,
//! whose `cargo test --workspace` takes it in. On a developer's Mac, under
//! `cargo xtask ci` too, it returns at once.

use std::fs;
use std::path::{Path, PathBuf};

use cupper_sys::{Registration, ensure_registered, registered_apps};

#[test]
fn launch_services_registers_a_bundle_once_on_a_runner() {
    if std::env::var_os("GITHUB_ACTIONS").is_none() {
        eprintln!("skipped: registers a bundle with LaunchServices, on a GitHub runner only");
        return;
    }
    // Under the target directory: LaunchServices marks an app in a temporary
    // directory as such (T101).
    let pid = std::process::id();
    let app = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("lsreg-{pid}/Probe.app"));
    let id = format!("dev.cupper.test.lsreg.p{pid}");
    fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    fs::copy("/usr/bin/true", app.join("Contents/MacOS/probe")).unwrap();
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>probe</string>
<key>CFBundleIdentifier</key><string>{id}</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>"#
    );
    fs::write(app.join("Contents/Info.plist"), plist).unwrap();

    assert_eq!(registered_apps(&id), [] as [PathBuf; 0]);
    assert_eq!(
        ensure_registered(&app, &id).unwrap(),
        Registration::Registered
    );
    let canonical = fs::canonicalize(&app).unwrap();
    let found = registered_apps(&id);
    assert!(
        found
            .iter()
            .any(|path| fs::canonicalize(path).is_ok_and(|path| path == canonical)),
        "{found:?}"
    );
    assert_eq!(ensure_registered(&app, &id).unwrap(), Registration::Already);
}
