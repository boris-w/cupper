#!/usr/bin/env node
// Prototype of cupper's first-run installer for pinned runtime components
// (SP-2 item 5). The real one would live in the host (Rust); this shows the
// flow and failure handling.
//
// Usage:
//   node install-runtime.mjs install <manifest.json> <runtimeRoot> [--esbuild-wasm <esbuild-wasm/lib/main.js>] [--fault <f>] [--only <name>]
//   node install-runtime.mjs verify  <manifest.json> <runtimeRoot> [--only <name>]
//
// Layout: <runtimeRoot>/<name>/<version>/ (+ .installed.json marker, written last)
// Faults (for the trial): bad-hash | bad-url | crash-before-commit
//
// Flow per component:
//   1. If <name>/<version>/.installed.json exists and every recorded file hash
//      matches, skip. If it does not match, move the dir aside (.broken-*).
//   2. Remove stale .staging-* dirs left by a crashed installer.
//   3. Download into .staging-*, hashing while streaming; compare with the
//      pinned sha256 / npm SRI *before* extracting anything.
//   4. Extract only what is needed; never run package scripts.
//   5. Optional local bundle step (esbuild-wasm) -> one adapter.mjs.
//   6. Hash every kept file, write .installed.json, then rename(2) the staging
//      dir to <version>/ (atomic on the same volume).
//   Any error: delete staging, keep whatever version was there before.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { createWriteStream, existsSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, relative } from "node:path";
import { performance } from "node:perf_hooks";

const [cmd, manifestPath, root, ...rest] = process.argv.slice(2);
const flag = (n) => {
  const i = rest.indexOf(`--${n}`);
  return i >= 0 ? rest[i + 1] : undefined;
};
const fault = flag("fault");
const only = flag("only");
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
const manifestDir = dirname(manifestPath);
const t0 = performance.now();
const log = (event, data = {}) => console.log(JSON.stringify({ t: Math.round(performance.now() - t0), event, ...data }));

class IntegrityError extends Error {}

function sriOf(buf, algo = "sha512") {
  return `${algo}-${createHash(algo).update(buf).digest("base64")}`;
}

async function download(url, file, expect) {
  // expect: { sha256 } or { integrity: "sha512-..." }
  const target = fault === "bad-url" ? url.replace(/^https:\/\/[^/]+/, "https://unreachable.invalid") : url;
  let lastErr;
  for (let attempt = 1; attempt <= 3; attempt++) {
    try {
      const res = await fetch(target, { headers: { "user-agent": "cupper-runtime-installer/0" } });
      if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`);
      const h256 = createHash("sha256");
      const h512 = createHash("sha512");
      const out = createWriteStream(file);
      for await (const chunk of res.body) {
        h256.update(chunk);
        h512.update(chunk);
        if (!out.write(chunk)) await new Promise((r) => out.once("drain", r));
      }
      await new Promise((r, j) => out.end((e) => (e ? j(e) : r())));
      const got = { sha256: h256.digest("hex"), integrity: `sha512-${h512.digest("base64")}` };
      const want = { ...expect };
      if (fault === "bad-hash") {
        if (want.sha256) want.sha256 = "0".repeat(64);
        if (want.integrity) want.integrity = "sha512-AAAA";
      }
      if (want.sha256 && want.sha256 !== got.sha256) throw new IntegrityError(`sha256 mismatch for ${url}: got ${got.sha256}`);
      if (want.integrity && want.integrity !== got.integrity) throw new IntegrityError(`integrity mismatch for ${url}`);
      return got;
    } catch (e) {
      if (e instanceof IntegrityError) throw e; // never retry a hash mismatch
      lastErr = e;
      log("download-retry", { url, attempt, error: e.cause?.code ?? e.message });
      await new Promise((r) => setTimeout(r, 300 * attempt));
    }
  }
  throw new Error(`download failed after 3 attempts: ${url}: ${lastErr?.cause?.code ?? lastErr?.message}`);
}

// bsdtar refuses absolute paths and ".." members by default; the Rust host
// would use a tar reader with the same checks.
function untar(archive, destDir, members = [], stripComponents = 0) {
  mkdirSync(destDir, { recursive: true });
  const args = ["-xf", archive, "-C", destDir];
  if (stripComponents) args.push(`--strip-components=${stripComponents}`);
  execFileSync("/usr/bin/tar", [...args, ...members], { stdio: ["ignore", "ignore", "pipe"] });
}

function walk(dir, base = dir, acc = []) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) walk(p, base, acc);
    else if (e.isFile()) acc.push(relative(base, p));
  }
  return acc;
}

function hashFiles(dir) {
  const files = {};
  for (const rel of walk(dir).sort()) {
    if (rel === ".installed.json") continue;
    files[rel] = createHash("sha256").update(readFileSync(join(dir, rel))).digest("hex");
  }
  return files;
}

function cleanStale(compDir) {
  if (!existsSync(compDir)) return;
  for (const e of readdirSync(compDir)) {
    if (e.startsWith(".staging-")) {
      rmSync(join(compDir, e), { recursive: true, force: true });
      log("stale-staging-removed", { dir: e });
    }
  }
}

function verifyDir(dir) {
  const marker = join(dir, ".installed.json");
  if (!existsSync(marker)) return { ok: false, reason: "no marker" };
  const m = JSON.parse(readFileSync(marker, "utf8"));
  const start = performance.now();
  for (const [rel, want] of Object.entries(m.files)) {
    const p = join(dir, rel);
    if (!existsSync(p)) return { ok: false, reason: `missing ${rel}` };
    const got = createHash("sha256").update(readFileSync(p)).digest("hex");
    if (got !== want) return { ok: false, reason: `hash mismatch ${rel}` };
  }
  return { ok: true, files: Object.keys(m.files).length, ms: Math.round(performance.now() - start) };
}

async function installOne(c) {
  const compDir = join(root, c.name);
  const finalDir = join(compDir, c.version);
  mkdirSync(compDir, { recursive: true });
  cleanStale(compDir);
  if (existsSync(finalDir)) {
    const v = verifyDir(finalDir);
    if (v.ok) return log("present", { name: c.name, version: c.version, verifyMs: v.ms });
    const aside = join(compDir, `.broken-${c.version}-${Date.now()}`);
    renameSync(finalDir, aside);
    log("broken-moved-aside", { name: c.name, reason: v.reason });
  }
  const staging = join(compDir, `.staging-${c.version}-${process.pid}`);
  const dl = join(staging, ".download");
  mkdirSync(dl, { recursive: true });
  const start = performance.now();
  try {
    if (c.kind === "archive-member") {
      // e.g. the official Node tarball: verify sha256, extract one member.
      const file = join(dl, "archive");
      await download(c.url, file, { sha256: c.sha256 });
      untar(file, join(staging, ".x"), [c.member]);
      mkdirSync(dirname(join(staging, c.dest)), { recursive: true });
      renameSync(join(staging, ".x", c.member), join(staging, c.dest));
      rmSync(join(staging, ".x"), { recursive: true });
      if (c.codesign) execFileSync("/usr/bin/codesign", ["--verify", "--strict", join(staging, c.dest)]);
    } else if (c.kind === "npm-member") {
      // A package whose published dist is already one self-contained file.
      const file = join(dl, "pkg.tgz");
      await download(c.url, file, { integrity: c.integrity });
      untar(file, join(staging, ".x"), [c.member, ...(c.keep ?? [])]);
      renameSync(join(staging, ".x", c.member), join(staging, c.dest));
      for (const k of c.keep ?? []) renameSync(join(staging, ".x", k), join(staging, k.split("/").pop()));
      rmSync(join(staging, ".x"), { recursive: true });
    } else if (c.kind === "npm-lockfile") {
      // Minimal `npm ci`: every non-optional, non-dev entry of a pinned
      // package-lock.json, verified against its SRI, no install scripts.
      const lock = JSON.parse(readFileSync(join(manifestDir, c.lockfile), "utf8"));
      const entries = Object.entries(lock.packages).filter(([k, v]) => k.startsWith("node_modules/") && !v.optional && !v.dev && !v.link);
      let i = 0;
      const worker = async () => {
        while (i < entries.length) {
          const [key, v] = entries[i++];
          const file = join(dl, `${key.replaceAll("/", "__")}.tgz`);
          await download(v.resolved, file, { integrity: v.integrity });
          untar(file, join(staging, key), [], 1);
        }
      };
      await Promise.all(Array.from({ length: 8 }, worker));
      log("lockfile-packages-installed", { name: c.name, packages: entries.length });
      if (c.bundle) {
        const esbuildMain = flag("esbuild-wasm");
        const esbuild = createRequire(esbuildMain)(esbuildMain);
        const b0 = performance.now();
        await esbuild.build({
          // Paths in the output are relative to absWorkingDir, so the result
          // does not depend on where the runtime root lives.
          absWorkingDir: staging,
          entryPoints: [join(staging, c.bundle.entry)],
          outfile: join(staging, c.bundle.dest),
          bundle: true,
          platform: "node",
          format: "esm",
          target: "node22",
          legalComments: "eof",
          logLevel: "warning",
          banner: { js: "import { createRequire as __cr } from 'node:module'; const require = __cr(import.meta.url);" },
        });
        const bundleSha256 = createHash("sha256").update(readFileSync(join(staging, c.bundle.dest))).digest("hex");
        log("bundled", { name: c.name, ms: Math.round(performance.now() - b0), bytes: statSync(join(staging, c.bundle.dest)).size, sha256: bundleSha256 });
        if (c.bundle.sha256 && c.bundle.sha256 !== bundleSha256) throw new IntegrityError(`local bundle hash ${bundleSha256} != pinned ${c.bundle.sha256}`);
        if (c.bundle.pruneTree) rmSync(join(staging, "node_modules"), { recursive: true });
      }
    } else {
      throw new Error(`unknown kind ${c.kind}`);
    }
    rmSync(dl, { recursive: true });
    const files = hashFiles(staging);
    writeFileSync(join(staging, ".installed.json"), JSON.stringify({ name: c.name, version: c.version, installedAt: new Date().toISOString(), files }, null, 1));
    if (fault === "crash-before-commit") {
      log("simulated-crash", { name: c.name });
      process.exit(9);
    }
    renameSync(staging, finalDir);
    log("installed", { name: c.name, version: c.version, files: Object.keys(files).length, ms: Math.round(performance.now() - start) });
  } catch (e) {
    rmSync(staging, { recursive: true, force: true });
    log("failed", { name: c.name, version: c.version, kind: e instanceof IntegrityError ? "integrity" : "other", error: e.message, stagingRemoved: !existsSync(staging), previousKept: existsSync(finalDir) ? "yes" : "none-installed" });
    process.exitCode = 1;
  }
}

const comps = manifest.components.filter((c) => !only || c.name === only);
if (cmd === "install") {
  for (const c of comps) await installOne(c);
} else if (cmd === "verify") {
  for (const c of comps) log("verify", { name: c.name, version: c.version, ...verifyDir(join(root, c.name, c.version)) });
} else {
  console.error("usage: install-runtime.mjs install|verify <manifest.json> <runtimeRoot> [...]");
  process.exit(2);
}
