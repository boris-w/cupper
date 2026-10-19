//! `rust`: formatting, clippy with warnings denied, the iOS build of
//! `cupper-protocol` and the workspace tests (design 5.1.6), plus the host's
//! other feature sets and its shuttle tests. A later task adds the golden tests.

use anyhow::Result;

use crate::ci::ctx::JobCtx;

/// The host's feature sets besides `--all-features` (the e2e build): the
/// release build (none, design 5.9.1), the self-use build (`dev-auth`) and the
/// integration-test build (`test-harness`). Code gated on these features is
/// compiled differently in each, so each gets its own clippy and test run.
const HOST_FEATURE_SETS: &[(&str, &[&str])] = &[
    ("release", &[]),
    ("dev-auth", &["dev-auth"]),
    ("test-harness", &["test-harness"]),
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
    for (name, features) in HOST_FEATURE_SETS {
        let mut clippy = ctx.cargo();
        clippy.args(["clippy", "-p", "cupper-host", "--all-targets", "--locked"]);
        with_features(&mut clippy, features);
        clippy.args(["--", "-D", "warnings"]);
        ctx.check(&format!("clippy-host-{name}"), &mut clippy)?;
    }

    // The library has no I/O and builds for iOS (design 5.1.1); its bin
    // `export-schema` stays out.
    let mut ios = ctx.cargo();
    ios.args(["build", "-p", "cupper-protocol", "--lib", "--locked"])
        .args(["--target", "aarch64-apple-ios-sim"]);
    ctx.check("ios-sim", &mut ios)?;

    let mut test = ctx.cargo();
    test.args([
        "test",
        "--workspace",
        "--all-features",
        "--locked",
        "--no-fail-fast",
    ]);
    ctx.libtest("test", &mut test)?;
    for (name, features) in HOST_FEATURE_SETS {
        let mut test = ctx.cargo();
        test.args(["test", "-p", "cupper-host", "--locked", "--no-fail-fast"]);
        with_features(&mut test, features);
        ctx.libtest(&format!("test-host-{name}"), &mut test)?;
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
    )
}

fn with_features(cmd: &mut std::process::Command, features: &[&str]) {
    if !features.is_empty() {
        cmd.args(["--features", &features.join(",")]);
    }
}
