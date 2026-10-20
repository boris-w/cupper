//! `rust`: formatting, clippy with warnings denied, the iOS build of
//! `cupper-protocol` and the workspace tests (design 5.1.6), plus the other
//! feature sets of the host and the native extension, the host's shuttle
//! tests, the golden tests (`pnpm test:golden`, A and B layers) and the
//! native extension's Node tests.

use anyhow::Result;

use crate::ci::ctx::JobCtx;

/// Feature sets besides `--all-features` (the e2e build): the host's release
/// build (none, design 5.9.1), self-use build (`dev-auth`) and
/// integration-test build (`test-harness`), and the native extension's
/// release build. Code gated on these features is compiled differently in
/// each, so each gets its own clippy and test run.
const FEATURE_SETS: &[(&str, &str, &[&str])] = &[
    ("cupper-host", "host-release", &[]),
    ("cupper-host", "host-dev-auth", &["dev-auth"]),
    ("cupper-host", "host-test-harness", &["test-harness"]),
    ("cupper-desktop-native", "native-release", &[]),
];

pub fn run(ctx: &mut JobCtx) -> Result<()> {
    let mut fmt = ctx.cargo();
    fmt.args(["fmt", "--all", "--check", "--", "--color", "never"]);
    ctx.check("fmt", &mut fmt)?;

    let mut clippy = ctx.cargo();
    clippy.args([
        "clippy",
        "--workspace",
        "--all-targets",
        "--all-features",
        "--locked",
        "--",
        "-D",
        "warnings",
    ]);
    ctx.check("clippy", &mut clippy)?;
    for (package, name, features) in FEATURE_SETS {
        let mut clippy = ctx.cargo();
        clippy.args(["clippy", "-p", package, "--all-targets", "--locked"]);
        with_features(&mut clippy, features);
        clippy.args(["--", "-D", "warnings"]);
        ctx.check(&format!("clippy-{name}"), &mut clippy)?;
    }

    // The library has no I/O and builds for iOS (design 5.1.1); its bin
    // `export-schema` stays out.
    let mut ios = ctx.cargo();
    ios.args(["build", "-p", "cupper-protocol", "--lib", "--locked"])
        .args(["--target", "aarch64-apple-ios-sim"]);
    ctx.check("ios-sim", &mut ios)?;

    // The host's tests run the fake agent from `target/debug` (T055).
    let mut fake_agent = ctx.cargo();
    fake_agent.args(["build", "-p", "cupper-fake-agent", "--locked"]);
    ctx.check("fake-agent", &mut fake_agent)?;
    // And the e2e build of the CLI with IT-elevate's probe: IT-cli-commands
    // runs `cupper`, the fake agent starts the probe (T133, T046).
    let mut cli = ctx.cargo();
    cli.args([
        "build",
        "-p",
        "cupper-cli",
        "--features",
        "test-harness",
        "--locked",
    ]);
    ctx.check("cli-e2e", &mut cli)?;
    let mut test = ctx.cargo();
    test.args([
        "test",
        "--workspace",
        "--all-features",
        "--locked",
        "--no-fail-fast",
    ]);
    ctx.libtest("test", &mut test)?;
    for (package, name, features) in FEATURE_SETS {
        let mut test = ctx.cargo();
        test.args(["test", "-p", package, "--locked", "--no-fail-fast"]);
        with_features(&mut test, features);
        ctx.libtest(&format!("test-{name}"), &mut test)?;
    }

    // PROP-stream-seq runs the host's streams under shuttle, which needs the
    // crate built with `--cfg shuttle` (design 5.9.8); a target dir of its own
    // keeps the other builds cached.
    let args = "test -p cupper-host --lib --locked --no-fail-fast --target-dir target/shuttle";
    let mut shuttle = ctx.cargo();
    shuttle
        .args(args.split(' '))
        .args(["--", "prop_stream_seq"]);
    ctx.libtest(
        "test-host-shuttle",
        shuttle.env("RUSTFLAGS", "--cfg shuttle"),
    )?;

    // UNIT-native: a Node process loads the extension's `dev-auth` build and
    // drives the gateway fixture with it; the package's test script builds
    // both first.
    let mut install = ctx.command("pnpm");
    install.args(["install", "--frozen-lockfile"]);
    ctx.check("pnpm-install", &mut install)?;
    ctx.libtest("golden", &mut golden(ctx))?;
    let mut native = ctx.command("pnpm");
    native.args(["--dir", "crates/cupper-desktop-native", "run", "test"]);
    ctx.vitest("test-native", &mut native)
}

/// The goldens of both layers (design 5.9.3), as `pnpm test:golden` runs
/// them for a developer.
fn golden(ctx: &JobCtx) -> std::process::Command {
    let mut golden = ctx.command("pnpm");
    golden.args(["run", "test:golden"]);
    golden
}

fn with_features(cmd: &mut std::process::Command, features: &[&str]) {
    if !features.is_empty() {
        cmd.args(["--features", &features.join(",")]);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    /// The `golden` step runs the root package's `test:golden`, which runs
    /// the host's GOLD-A and GOLD-B tests: their names start with `gold_`.
    #[test]
    fn gold_b_fake_runs_in_the_rust_job() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let ctx = JobCtx::new(&root, &root, "rust");
        let step = golden(&ctx);
        let args: Vec<_> = step.get_args().filter_map(|a| a.to_str()).collect();
        assert_eq!(step.get_program(), "pnpm");
        assert_eq!(args, ["run", "test:golden"]);
        let package = std::fs::read_to_string(root.join("package.json")).unwrap();
        let package: serde_json::Value = serde_json::from_str(&package).unwrap();
        let script = "cargo test -p cupper-host --features test-harness --locked gold_";
        assert_eq!(package["scripts"]["test:golden"], script);
        let source = include_str!("rust.rs");
        assert!(source.contains("ctx.libtest(\"golden\", &mut golden(ctx))?;"));
    }
}
