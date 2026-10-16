#!/usr/bin/env node
// License inventory of an installed node_modules tree.
// Usage: node licenses.mjs <dir containing node_modules> [--lock <package-lock.json>]
// Prints JSON: one entry per installed package with the declared license, the
// license/notice files shipped in the package, a category, and (with --lock)
// whether the lockfile marks it optional/peer.

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.argv[2];
const lockIdx = process.argv.indexOf("--lock");
const lock = lockIdx > 0 ? JSON.parse(readFileSync(process.argv[lockIdx + 1], "utf8")) : null;

const PERMISSIVE = /^(MIT|ISC|BSD-2-Clause|BSD-3-Clause|Apache-2\.0|0BSD|BlueOak-1\.0\.0|Unlicense|CC0-1\.0|Python-2\.0|Zlib)$/;
const WEAK = /^(MPL-2\.0|LGPL-|EPL-)/;
const STRONG = /^(GPL-|AGPL-)/;

function categorize(expr) {
  if (!expr) return "missing";
  if (/^SEE LICENSE IN/i.test(expr) || /UNLICENSED/i.test(expr)) return "custom";
  const parts = expr.replace(/[()]/g, "").split(/\s+(?:OR|AND)\s+/);
  const isOr = /\sOR\s/.test(expr);
  const cats = parts.map((p) => (PERMISSIVE.test(p) ? "permissive" : WEAK.test(p) ? "weak-copyleft" : STRONG.test(p) ? "strong-copyleft" : "unknown"));
  if (isOr && cats.includes("permissive")) return "permissive";
  for (const c of ["strong-copyleft", "weak-copyleft", "unknown"]) if (cats.includes(c)) return c;
  return "permissive";
}

function* packages(nm, prefix = "node_modules") {
  if (!existsSync(nm)) return;
  for (const e of readdirSync(nm)) {
    if (e.startsWith(".")) continue;
    if (e.startsWith("@")) {
      for (const s of readdirSync(join(nm, e))) yield* one(join(nm, e, s), `${prefix}/${e}/${s}`);
    } else yield* one(join(nm, e), `${prefix}/${e}`);
  }
}
function* one(dir, key) {
  const pj = join(dir, "package.json");
  if (!existsSync(pj)) return;
  yield { dir, key };
  yield* packages(join(dir, "node_modules"), `${key}/node_modules`);
}

const rows = [];
for (const { dir, key } of packages(join(root, "node_modules"))) {
  const p = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
  let lic = p.license;
  if (lic && typeof lic === "object") lic = lic.type;
  if (!lic && Array.isArray(p.licenses)) lic = p.licenses.map((l) => l.type ?? l).join(" OR ");
  const files = readdirSync(dir).filter((f) => /^(licen[cs]e|copying|notice|third.?party)/i.test(f));
  const l = lock?.packages?.[key];
  rows.push({
    name: p.name,
    version: p.version,
    license: lic ?? null,
    category: categorize(lic),
    files,
    hasNotice: files.some((f) => /^notice/i.test(f)),
    optional: Boolean(l?.optional),
    peer: Boolean(l?.peer),
  });
}
rows.sort((a, b) => a.name.localeCompare(b.name));
console.log(JSON.stringify(rows, null, 1));
