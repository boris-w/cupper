#!/usr/bin/env node
// Bundle an adapter entry point into one ESM file with esbuild, then check that
// the result only imports Node builtins (so nothing is lazily loaded from disk
// after start-up).
//
// Usage:
//   node bundle.mjs --esbuild <esbuild binary> --entry <dist/index.js> --out <file.mjs>
//        [--format esm|cjs] [--alias NAME=PATH]
//   node bundle.mjs --check <file>          # only run the import check
//
// Prints a JSON summary: size, non-builtin static/dynamic imports and requires.

import { execFileSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";
import { builtinModules } from "node:module";

const argv = process.argv.slice(2);
const opt = (name) => {
  const i = argv.indexOf(`--${name}`);
  return i >= 0 ? argv[i + 1] : undefined;
};

const out = opt("check") ?? opt("out");
if (!opt("check")) {
  const format = opt("format") ?? "esm";
  const args = [
    opt("entry"),
    "--bundle",
    "--platform=node",
    "--target=node22",
    `--format=${format}`,
    `--outfile=${out}`,
    "--legal-comments=eof",
    "--log-level=warning",
  ];
  // CommonJS dependencies inside an ESM bundle still call require() for
  // builtins; give them one.
  if (format === "esm") args.push("--banner:js=import { createRequire as __cr } from 'node:module'; const require = __cr(import.meta.url);");
  // In a CommonJS bundle import.meta is empty; the SDK calls
  // createRequire(import.meta.url), so provide a file URL for it.
  if (format === "cjs") {
    args.push("--define:import.meta.url=__import_meta_url");
    args.push("--banner:js=const __import_meta_url = require('node:url').pathToFileURL(__filename).href;");
  }
  if (opt("alias")) args.push(`--alias:${opt("alias")}`);
  execFileSync(opt("esbuild"), args, { stdio: "inherit" });
}

const src = readFileSync(out, "utf8");
const builtin = new Set([...builtinModules, ...builtinModules.map((m) => `node:${m}`)]);
const isBuiltin = (s) => builtin.has(s) || builtin.has(s.split("/")[0]) || s.startsWith("node:");
const collect = (re) => [...new Set([...src.matchAll(re)].map((m) => m[1]))].filter((s) => !isBuiltin(s));
const summary = {
  file: out.split("/").pop(),
  bytes: statSync(out).size,
  nonBuiltinStaticImports: collect(/(?:^|[;\n])\s*import\s[^;]*?from\s*["']([^"']+)["']/g),
  nonBuiltinDynamicImports: collect(/import\(\s*["']([^"']+)["']\s*\)/g),
  nonBuiltinRequires: collect(/require\(\s*["']([^"']+)["']\s*\)/g),
  requireResolve: collect(/require\d*\.resolve\(\s*["']([^"']+)["']\s*\)/g),
  importMetaResolve: collect(/import\.meta\.resolve\(\s*["']([^"']+)["']\s*\)/g),
};
console.log(JSON.stringify(summary, null, 1));
