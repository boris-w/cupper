//! `release-config` (nightly, T161): builds the release configuration
//! without a Developer ID and checks the app it built (`apps/desktop/release`):
//! the fuses, the asar archive, Info.plist, the test entry points,
//! `--build-info`, and E2E-SEC-switches; then `runtime-pack --verify`.
//! Cupper Local gets the same scan in `selfuse`, which builds it anyway.

use anyhow::Result;

use super::e2e;
use crate::ci::ctx::{Artifact, JobCtx};

const APP: &str = "apps/desktop/dist/pack/release/mac-arm64/Cupper.app";

pub fn run(ctx: &mut JobCtx) -> Result<()> {
    e2e::prepare(ctx)?;
    // No identity, the keychain untouched; the app alone, not its dmg and zip.
    let mut build = ctx.command("pnpm");
    build
        .args(["--dir=apps/desktop", "run", "build:release"])
        .args(["-c.mac.identity=null", "-c.mac.target=dir"])
        .env("CSC_IDENTITY_AUTO_DISCOVERY", "false");
    ctx.check("build", &mut build)?;
    // The fuses broke the framework's linker signature, without which arm64
    // kills the app as it loads it: signed again ad hoc, no identity.
    let mut sign = ctx.command("codesign");
    sign.args(["--force", "--sign", "-"]).arg(
        ctx.root
            .join(APP)
            .join("Contents/Frameworks/Electron Framework.framework"),
    );
    ctx.check("sign-ad-hoc", &mut sign)?;
    let sha256 = e2e::digest(&ctx.root.join(APP))?;
    ctx.artifacts.push(Artifact {
        name: "Cupper.app".to_owned(),
        sha256,
    });

    let mut test = ctx.command("pnpm");
    test.args(["--dir=apps/desktop", "run", "test:release"])
        .env("CUPPER_RELEASE_APP", ctx.root.join(APP));
    let tested = ctx.vitest("test", &mut test);
    let mut verify = ctx.cargo();
    verify.args(["xtask", "runtime-pack", "--verify"]);
    let verified = ctx.check("runtime-verify", &mut verify);
    tested.and(verified)
}
