// Before/after fingerprint of the user's Claude configuration (brief: security
// constraint "运行前后做哈希对比").
//
//   node config-hash.ts snapshot <out.json>
//   node config-hash.ts compare <before.json> <after.json> [--base <tmp dir>]
//
// The snapshot holds only SHA-256 digests, keyed by field name (and, for
// per-project fields, by project path), so it never copies a setting's value.
// It still contains local paths, so keep it outside the repository.
// `compare` prints field names and "changed / unchanged" only.

import { createHash } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

type Json = any;

const SETTINGS_FILES = ["settings.json", "settings.local.json"];

// Top-level fields of ~/.claude.json that grant or relax permissions, or start
// programs (MCP servers). Any other top-level key whose name looks
// permission-related is picked up by PERMISSION_KEY as well.
const TOP_FIELDS = ["mcpServers", "bypassPermissionsModeAccepted", "autoModeEnvSetup"];
const PERMISSION_KEY = /permission|bypass|automode|trust|allowed/i;

// Per-project fields of ~/.claude.json with the same meaning.
const PROJECT_FIELDS = [
  "allowedTools",
  "hasTrustDialogAccepted",
  "mcpServers",
  "enabledMcpServers",
  "disabledMcpServers",
  "enabledMcpjsonServers",
  "disabledMcpjsonServers",
  "hasClaudeMdExternalIncludesApproved",
];

function digest(value: unknown): string {
  if (value === undefined) return "absent";
  return createHash("sha256").update(JSON.stringify(value)).digest("hex");
}

function snapshot(): Json {
  const claudeDir = join(homedir(), ".claude");
  const files: Record<string, string> = {};
  for (const name of SETTINGS_FILES) {
    const p = join(claudeDir, name);
    files[`~/.claude/${name}`] = existsSync(p) ? createHash("sha256").update(readFileSync(p)).digest("hex") : "absent";
  }
  const state = JSON.parse(readFileSync(join(homedir(), ".claude.json"), "utf8"));
  const top: Record<string, string> = {};
  const topNames = new Set([...TOP_FIELDS, ...Object.keys(state).filter((k) => PERMISSION_KEY.test(k))]);
  for (const k of topNames) top[k] = digest(state[k]);
  const projects: Record<string, Record<string, string>> = {};
  for (const [path, proj] of Object.entries<Json>(state.projects ?? {})) {
    projects[path] = {};
    for (const f of PROJECT_FIELDS) projects[path][f] = digest(proj?.[f]);
  }
  return { takenAt: new Date().toISOString(), files, top, projects };
}

/** Whether a new project entry only carries default (empty or false) permission fields. */
function defaultsOnly(path: string): boolean {
  const state = JSON.parse(readFileSync(join(homedir(), ".claude.json"), "utf8"));
  const proj = state.projects?.[path] ?? {};
  return PROJECT_FIELDS.every((f) => {
    const v = proj[f];
    return v === undefined || v === false || (Array.isArray(v) && v.length === 0) || (v && typeof v === "object" && Object.keys(v).length === 0);
  });
}

function compare(before: Json, after: Json, base?: string) {
  const lines: string[] = [];
  for (const k of Object.keys({ ...before.files, ...after.files })) {
    lines.push(`${k}: ${before.files[k] === after.files[k] ? "unchanged" : "CHANGED"}${before.files[k] === "absent" && after.files[k] === "absent" ? " (absent before and after)" : ""}`);
  }
  for (const k of Object.keys({ ...before.top, ...after.top }).sort()) {
    lines.push(`~/.claude.json ${k}: ${before.top[k] === after.top[k] ? "unchanged" : "CHANGED"}`);
  }
  const shared = Object.keys(before.projects).filter((p) => p in after.projects);
  for (const f of PROJECT_FIELDS) {
    const changed = shared.filter((p) => before.projects[p][f] !== after.projects[p][f]);
    lines.push(`~/.claude.json projects.*.${f} (${shared.length} existing projects): ${changed.length === 0 ? "unchanged" : `CHANGED in ${changed.length} project(s)`}`);
  }
  const removed = Object.keys(before.projects).filter((p) => !(p in after.projects));
  const added = Object.keys(after.projects).filter((p) => !(p in before.projects));
  const ours = base ? added.filter((p) => p.startsWith(base) || p.startsWith("/private" + base)) : [];
  lines.push(`~/.claude.json projects: ${added.length} added (${ours.length} under this run's temp dir), ${removed.length} removed`);
  for (const p of ours) lines.push(`  temp-dir project entry: permission fields ${defaultsOnly(p) ? "all at defaults (empty or false)" : "NOT at defaults"}`);
  console.log(lines.join("\n"));
}

const [mode, a, b] = process.argv.slice(2);
if (mode === "snapshot" && a) {
  writeFileSync(a, JSON.stringify(snapshot(), null, 1), { mode: 0o600 });
  console.log(`snapshot written`);
} else if (mode === "compare" && a && b) {
  const i = process.argv.indexOf("--base");
  compare(JSON.parse(readFileSync(a, "utf8")), JSON.parse(readFileSync(b, "utf8")), i > 0 ? process.argv[i + 1] : undefined);
} else {
  console.error("usage: node config-hash.ts snapshot <out.json> | compare <before.json> <after.json> [--base <dir>]");
  process.exit(2);
}
