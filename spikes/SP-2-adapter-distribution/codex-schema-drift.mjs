#!/usr/bin/env node
// Compare the Codex app-server protocol across codex binaries, restricted to the
// methods a given codex-acp build actually references.
//
// Usage:
//   node codex-schema-drift.mjs --adapter <codex-acp/dist/index.js> --out <tmpdir> \
//        --baseline <label> <label>=<codex binary> [<label>=<codex binary> ...]
//
// For every binary it runs `codex app-server generate-json-schema` twice (with and
// without --experimental) into <tmpdir>/<label>/{exp,stable}. Then, for every
// method/notification name that appears as a string literal in the adapter, it
// reports per version: present?, stable or experimental?, and whether the
// transitive schema of its params differs from the baseline (with the names of
// the definitions that changed and the property keys added/removed).
// Prints JSON to stdout.

import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { join } from "node:path";

const argv = process.argv.slice(2);
const opt = (name) => {
  const i = argv.indexOf(`--${name}`);
  return i >= 0 ? argv.splice(i, 2)[1] : undefined;
};
const adapter = opt("adapter");
const out = opt("out");
const baseline = opt("baseline");
const versions = argv.map((a) => {
  const i = a.indexOf("=");
  return { label: a.slice(0, i), bin: a.slice(i + 1) };
});

const FILES = { request: "ClientRequest.json", notification: "ServerNotification.json", serverRequest: "ServerRequest.json" };

function generate(v) {
  for (const flavor of ["exp", "stable"]) {
    const dir = join(out, v.label, flavor);
    mkdirSync(dir, { recursive: true });
    const args = ["app-server", "generate-json-schema", "--out", dir];
    if (flavor === "exp") args.push("--experimental");
    execFileSync(v.bin, args, { stdio: "ignore" });
  }
  v.version = execFileSync(v.bin, ["--version"], { encoding: "utf8" }).trim();
}

function load(v, flavor) {
  const res = {};
  for (const [kind, file] of Object.entries(FILES)) {
    const j = JSON.parse(readFileSync(join(out, v.label, flavor, file), "utf8"));
    for (const variant of j.oneOf ?? []) {
      const method = variant.properties?.method?.enum?.[0];
      if (!method) continue;
      const ref = variant.properties?.params?.$ref?.replace("#/definitions/", "") ?? null;
      res[method] = { kind, ref, defs: j.definitions ?? {} };
    }
  }
  return res;
}

function closure(defs, root) {
  const seen = new Map();
  const walk = (name) => {
    if (!name || seen.has(name) || !defs[name]) return;
    seen.set(name, defs[name]);
    JSON.stringify(defs[name], (k, val) => {
      if (k === "$ref" && typeof val === "string") walk(val.replace("#/definitions/", ""));
      return val;
    });
  };
  walk(root);
  return seen;
}

const canon = (x) => JSON.stringify(x, (k, v) => (v && typeof v === "object" && !Array.isArray(v) ? Object.fromEntries(Object.entries(v).filter(([kk]) => kk !== "description").sort()) : v));
const hash = (s) => createHash("sha256").update(s).digest("hex").slice(0, 12);

function propKeys(def) {
  const keys = new Set(Object.keys(def?.properties ?? {}));
  for (const alt of def?.oneOf ?? def?.anyOf ?? []) for (const k of Object.keys(alt.properties ?? {})) keys.add(k);
  return keys;
}
function enumValues(def) {
  const vals = new Set(def?.enum ?? []);
  for (const alt of def?.oneOf ?? def?.anyOf ?? []) {
    for (const e of alt.enum ?? []) vals.add(e);
    for (const p of Object.values(alt.properties ?? {})) for (const e of p.enum ?? []) vals.add(e);
  }
  return vals;
}

function diffClosure(a, b) {
  const changed = [];
  for (const name of new Set([...a.keys(), ...b.keys()])) {
    const da = a.get(name);
    const db = b.get(name);
    if (!da || !db) {
      changed.push({ def: name, change: da ? "removed" : "added" });
      continue;
    }
    if (canon(da) === canon(db)) continue;
    const ka = propKeys(da), kb = propKeys(db);
    const ea = enumValues(da), eb = enumValues(db);
    const entry = { def: name };
    const plus = [...kb].filter((k) => !ka.has(k)), minus = [...ka].filter((k) => !kb.has(k));
    if (plus.length) entry.propsAdded = plus;
    if (minus.length) entry.propsRemoved = minus;
    const eplus = [...eb].filter((k) => !ea.has(k)), eminus = [...ea].filter((k) => !eb.has(k));
    if (eplus.length) entry.enumAdded = eplus;
    if (eminus.length) entry.enumRemoved = eminus;
    const ra = new Set(da.required ?? []), rb = new Set(db.required ?? []);
    const rplus = [...rb].filter((k) => !ra.has(k));
    if (rplus.length) entry.newlyRequired = rplus;
    changed.push(entry);
  }
  return changed;
}

for (const v of versions) generate(v);
const loaded = Object.fromEntries(versions.map((v) => [v.label, { exp: load(v, "exp"), stable: load(v, "stable") }]));

const src = readFileSync(adapter, "utf8");
const allMethods = new Set(versions.flatMap((v) => Object.keys(loaded[v.label].exp)));
const used = [...allMethods].filter((m) => src.includes(`"${m}"`)).sort();

const base = loaded[baseline].exp;
const report = { versions: versions.map((v) => ({ label: v.label, version: v.version })), baseline, usedMethodCount: used.length, methods: [] };
for (const m of used) {
  const row = { method: m, kind: base[m]?.kind ?? null, perVersion: {} };
  const baseClosure = base[m] ? closure(base[m].defs, base[m].ref) : new Map();
  for (const v of versions) {
    const e = loaded[v.label].exp[m];
    if (!e) {
      row.perVersion[v.label] = { present: false };
      continue;
    }
    const c = closure(e.defs, e.ref);
    const stable = Boolean(loaded[v.label].stable[m]);
    const cell = { present: true, stable, paramsHash: hash(canon([...c.entries()].sort())) };
    if (v.label !== baseline && base[m]) {
      const d = diffClosure(baseClosure, c);
      if (d.length) cell.changedDefs = d;
    }
    row.perVersion[v.label] = cell;
  }
  report.methods.push(row);
}
const methodsByVersion = Object.fromEntries(versions.map((v) => [v.label, { all: Object.keys(loaded[v.label].exp).length, stable: Object.keys(loaded[v.label].stable).length }]));
report.methodCounts = methodsByVersion;
console.log(JSON.stringify(report, null, 1));
