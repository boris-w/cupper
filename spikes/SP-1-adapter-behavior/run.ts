// Entry point: node run.ts --base <temp dir> --adapter <claude|codex-bundled|codex-local> --scenarios s01,s02
//
// <base> must already contain the adapters, installed with npm:
//   <base>/adapters/claude  -> @agentclientprotocol/claude-agent-acp
//   <base>/adapters/codex   -> @agentclientprotocol/codex-acp
// Sessions run in <base>/repos/<adapter> (a throwaway git repo created here);
// raw, unsanitized recordings go to <base>/raw/<adapter>/.

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { SCENARIOS } from "./scenarios.ts";
import type { Ctx, Family } from "./scenarios.ts";
import { CLIENT_CAPABILITIES, LIVE } from "./acp-client.ts";

function arg(name: string, fallback?: string): string {
  const i = process.argv.indexOf(`--${name}`);
  if (i >= 0 && process.argv[i + 1]) return process.argv[i + 1];
  if (fallback !== undefined) return fallback;
  throw new Error(`missing --${name}`);
}

const base = arg("base");
const adapter = arg("adapter");
const scenarioIds = arg("scenarios", Object.keys(SCENARIOS).join(",")).split(",");
const family: Family = adapter.startsWith("claude") ? "claude" : "codex";

const pkgRoot = family === "claude"
  ? join(base, "adapters/claude/node_modules/@agentclientprotocol/claude-agent-acp")
  : join(base, "adapters/codex/node_modules/@agentclientprotocol/codex-acp");
const entry = join(pkgRoot, "dist/index.js");
if (!existsSync(entry)) throw new Error(`adapter not installed: ${entry}`);

function which(cmd: string): string {
  return execFileSync("/usr/bin/which", [cmd], { encoding: "utf8" }).trim();
}
function version(cmd: string, args: string[]): string {
  try {
    return execFileSync(cmd, args, { encoding: "utf8", timeout: 20_000 }).trim();
  } catch {
    return "unknown";
  }
}
function pkgVersion(dir: string): string {
  try {
    return JSON.parse(readFileSync(join(dir, "package.json"), "utf8")).version;
  } catch {
    return "unknown";
  }
}

// Only a small allowlist of the parent environment reaches the adapter, so no
// session variables of the calling tool (or secrets) leak into the agent.
const KEEP_ENV = ["HOME", "PATH", "SHELL", "TMPDIR", "LANG", "LC_ALL", "LC_CTYPE", "USER", "LOGNAME"];
function baseEnv(): Record<string, string> {
  const env: Record<string, string> = {};
  for (const k of KEEP_ENV) if (process.env[k]) env[k] = process.env[k]!;
  return env;
}

const cliEnv: Record<string, string> = {};
let cliVersion = "bundled";
if (adapter === "claude") {
  cliEnv.CLAUDE_CODE_EXECUTABLE = process.env.SP1_CLAUDE_BIN ?? which("claude");
  cliVersion = version(cliEnv.CLAUDE_CODE_EXECUTABLE, ["--version"]);
} else if (adapter === "codex-local") {
  cliEnv.CODEX_PATH = process.env.SP1_CODEX_BIN ?? which("codex");
  cliVersion = version(cliEnv.CODEX_PATH, ["--version"]);
} else if (adapter === "codex-bundled") {
  cliVersion = `@openai/codex ${pkgVersion(join(base, "adapters/codex/node_modules/@openai/codex"))}`;
} else {
  throw new Error(`unknown adapter ${adapter}`);
}

const versions = {
  adapter: `${family === "claude" ? "@agentclientprotocol/claude-agent-acp" : "@agentclientprotocol/codex-acp"} ${pkgVersion(pkgRoot)}`,
  acpSdk: pkgVersion(join(base, `adapters/${family}/node_modules/@agentclientprotocol/sdk`)),
  agentSdk: family === "claude" ? pkgVersion(join(base, "adapters/claude/node_modules/@anthropic-ai/claude-agent-sdk")) : undefined,
  cli: cliVersion,
  node: process.version,
};

// Throwaway repo, outside any user repository.
const repo = join(base, "repos", adapter);
if (!existsSync(join(repo, ".git"))) {
  mkdirSync(repo, { recursive: true });
  writeFileSync(join(repo, "app.js"), "function greet(name) {\n  return `Hello, ${name}!`;\n}\n\nmodule.exports = { greet };\n");
  writeFileSync(join(repo, "README.md"), "# sp1 sandbox\n\nThrowaway repository for adapter tests.\n");
  const git = (...a: string[]) => execFileSync("git", a, { cwd: repo, stdio: "ignore" });
  git("init", "-q", "-b", "main");
  git("config", "user.name", "cupper-sp1");
  git("config", "user.email", "sp1@example.invalid");
  git("config", "commit.gpgsign", "false");
  git("add", ".");
  git("commit", "-q", "-m", "init");
}

const rawDir = join(base, "raw", adapter);
mkdirSync(rawDir, { recursive: true });
const statePath = join(rawDir, "state.json");
const state = existsSync(statePath) ? JSON.parse(readFileSync(statePath, "utf8")) : {};

const ctx: Ctx = {
  adapter,
  family,
  base,
  repo,
  rawDir,
  state,
  saveState: () => writeFileSync(statePath, JSON.stringify(state, null, 2)),
  spec: (extraEnv) => ({
    name: adapter,
    command: process.execPath,
    args: [entry],
    env: { ...baseEnv(), ...cliEnv, ...(extraEnv ?? {}) },
  }),
  header: (scenario) => ({
    scenario,
    adapter,
    versions,
    date: new Date().toISOString().slice(0, 10),
    clientCapabilities: CLIENT_CAPABILITIES,
  }),
};

for (const id of scenarioIds) {
  const fn = SCENARIOS[id];
  if (!fn) throw new Error(`unknown scenario ${id}`);
  const t = Date.now();
  console.log(`[${adapter}] ${id} start`);
  try {
    await fn(ctx);
    console.log(`[${adapter}] ${id} done in ${Math.round((Date.now() - t) / 1000)}s`);
  } catch (e) {
    console.log(`[${adapter}] ${id} FAILED after ${Math.round((Date.now() - t) / 1000)}s: ${(e as Error).message}`);
    for (const c of [...LIVE]) {
      c.note("scenario-failed", { message: (e as Error).message });
      await c.stop();
      c.close();
    }
  }
}
process.exit(0);
