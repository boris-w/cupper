//! The workspace `clippy.toml` must reject every time and UUIDv7 call that bypasses
//! `Clock` / `IdGen` (design 5.2.16). The fixture crate calls each listed method once.

use std::path::Path;
use std::process::Command;

#[test]
fn clippy_rejects_every_disallowed_method() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.join("../..");
    let config = std::fs::read_to_string(root.join("clippy.toml")).expect("read clippy.toml");
    let paths = disallowed_paths(&config);
    assert!(
        paths.len() >= 13,
        "expected the design 5.2.16 list, got {paths:?}"
    );

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args(["clippy", "--quiet", "--locked", "--", "-D", "warnings"])
        .current_dir(manifest_dir.join("tests/fixtures/disallowed-methods"))
        .env("CLIPPY_CONF_DIR", &root)
        .env(
            "CARGO_TARGET_DIR",
            Path::new(env!("CARGO_TARGET_TMPDIR")).join("disallowed-methods"),
        )
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("run cargo clippy");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "clippy accepted the negative sample:\n{stderr}"
    );
    for path in &paths {
        assert!(
            stderr.contains(&format!("use of a disallowed method `{path}`")),
            "no diagnostic for `{path}`; add a call to the fixture or fix the path:\n{stderr}"
        );
    }
}

/// The `path = "..."` entries of `disallowed-methods`.
fn disallowed_paths(config: &str) -> Vec<String> {
    config
        .lines()
        .filter_map(|line| line.split_once("path = \"")?.1.split_once('"'))
        .map(|(path, _)| path.to_owned())
        .collect()
}
