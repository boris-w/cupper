//! `ts`: Prettier, ESLint, `tsc -b` and the package tests (design 5.1.6). Until
//! the first TS package exists, only the root configuration files are checked.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::ci::ctx::JobCtx;

pub fn run(ctx: &mut JobCtx) -> Result<()> {
    let mut install = ctx.command("pnpm");
    install.args(["install", "--frozen-lockfile"]);
    ctx.check("install", &mut install)?;

    let mut prettier = ctx.command("pnpm");
    prettier.args(["exec", "prettier", "--check", "."]);
    ctx.check("prettier", &mut prettier)?;

    let mut eslint = ctx.command("pnpm");
    eslint.args(["exec", "eslint", "--max-warnings=0", "."]);
    ctx.check("eslint", &mut eslint)?;

    let projects = projects(ctx.root)?;
    if projects.is_empty() {
        eprintln!("  (no TS packages yet: tsc and tests skipped)");
        return Ok(());
    }
    let mut tsc = ctx.command("pnpm");
    tsc.args(["exec", "tsc", "--build"]).args(&projects);
    ctx.check("tsc", &mut tsc)?;

    // A package's `test` script runs Vitest; the job adds the JSON reporter,
    // so that every test case reaches the summary. Every package runs, even
    // after one fails.
    let mut failures = Vec::new();
    for project in &projects {
        if !has_test_script(&ctx.root.join(project))? {
            continue;
        }
        let name = project.rsplit('/').next().unwrap_or(project);
        let mut test = ctx.command("pnpm");
        test.args(["--dir", project, "run", "test"]);
        if let Err(err) = ctx.vitest(&format!("test-{name}"), &mut test) {
            failures.push(format!("{err:#}"));
        }
    }
    if !failures.is_empty() {
        bail!("{}", failures.join("\n  "));
    }
    Ok(())
}

/// Workspace packages (`packages/*`, `apps/*`, `tools/*`) with a `tsconfig.json`.
fn projects(root: &Path) -> Result<Vec<String>> {
    let mut found = Vec::new();
    for group in ["packages", "apps", "tools"] {
        let Ok(entries) = fs::read_dir(root.join(group)) else {
            continue;
        };
        for entry in entries {
            let path = entry?.path();
            if path.join("package.json").is_file() && path.join("tsconfig.json").is_file() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                found.push(format!("{group}/{name}"));
            }
        }
    }
    found.sort();
    Ok(found)
}

fn has_test_script(project: &Path) -> Result<bool> {
    let path = project.join("package.json");
    let manifest: Value = fs::read_to_string(&path)
        .map_err(anyhow::Error::from)
        .and_then(|text| Ok(serde_json::from_str(&text)?))
        .with_context(|| format!("cannot read {}", path.display()))?;
    Ok(manifest["scripts"]["test"].is_string())
}
