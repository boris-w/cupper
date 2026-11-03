// What the self-use checks share (T160): the apps that `cargo xtask ci
// selfuse` built, started as a user starts them, and what they leave in
// their real data and log directories. Only a GitHub-hosted runner runs
// them: on the author's Mac those directories hold Cupper Local in daily use.

import { execFileSync, spawn, spawnSync } from "node:child_process";
import { existsSync, readFileSync, realpathSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

export const HOST_APP = "Contents/Helpers/CupperHost.app";
export const HOST = `${HOST_APP}/Contents/MacOS/cupper-host`;
export const CLI = "Contents/Helpers/cupper";
/** An Electron helper of Cupper Local: `kind` is "", " (GPU)", " (Renderer)" or " (Plugin)". */
export const helper = (kind: string) =>
  `Contents/Frameworks/Cupper Local Helper${kind}.app/Contents/MacOS/Cupper Local Helper${kind}`;

type Built = "CUPPER_SELFUSE_APP" | "CUPPER_SELFUSE_NEXT_APP" | "CUPPER_DEV_APP";

/** The app the job built and names in `variable`. */
export function built(variable: Built): string {
  if (process.env.RUNNER_ENVIRONMENT !== "github-hosted") {
    throw new Error(
      "the self-use checks run on GitHub-hosted runners only: cargo xtask ci selfuse",
    );
  }
  const app = process.env[variable];
  if (!app) {
    throw new Error(`${variable} is not set: cargo xtask ci selfuse sets it`);
  }
  return realpathSync(app);
}

export const data = (name: string) => join(homedir(), "Library/Application Support", name);
export const logs = (name: string) => join(homedir(), "Library/Logs", name);

/** A plist file as JSON; `-` reads `input`. */
export function plist(file: string, input?: string): Record<string, unknown> {
  const args = ["-convert", "json", "-o", "-", "--", file];
  return JSON.parse(execFileSync("plutil", args, { input, encoding: "utf8" })) as Record<
    string,
    unknown
  >;
}

/** `codesign --display` of a file or a running pid: its `Key=value` lines, and what `args` print. */
export function codesign(target: string | number, ...args: string[]) {
  const run = spawnSync("codesign", ["--display", "--verbose=3", ...args, String(target)], {
    encoding: "utf8",
  });
  if (run.status !== 0) {
    throw new Error(`codesign ${String(target)}: ${run.stderr}`);
  }
  return { details: run.stderr, out: run.stdout };
}

export function field(details: string, key: string): string | undefined {
  return new RegExp(`^${key}=(.*)$`, "m").exec(details)?.[1];
}

export const cdhash = (target: string | number) => field(codesign(target).details, "CDHash");

/** The CLI in `app`, on its default data directory. */
export const cli = (app: string, ...args: string[]) =>
  spawnSync(join(app, CLI), args, { encoding: "utf8", timeout: 30_000 });

export const size = (file: string) => (existsSync(file) ? statSync(file).size : 0);

/** The JSON lines `file` got after its first `from` bytes. */
export function linesSince(file: string, from: number): Record<string, unknown>[] {
  const text = existsSync(file) ? readFileSync(file).subarray(from).toString() : "";
  return text
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line) as Record<string, unknown>);
}

/** What `check` returns once it returns anything; what it throws ends the wait. */
export async function until<T>(what: string, check: () => T | undefined, ms = 60_000) {
  const end = Date.now() + ms;
  for (;;) {
    const found = check();
    if (found !== undefined) {
      return found;
    }
    if (Date.now() > end) {
      throw new Error(`no ${what} within ${String(ms)} ms`);
    }
    await new Promise((done) => setTimeout(done, 250));
  }
}

/** The executable of each process of this user: `ps` prints `comm` as the whole path on macOS. */
export function processes(): Map<number, string> {
  const out = execFileSync("ps", ["-x", "-o", "pid=,comm="], { encoding: "utf8" });
  const rows = out.split("\n").map((line) => /^\s*(\d+)\s+(.*)$/.exec(line));
  return new Map(
    rows.filter((row) => row !== null).map(([, pid, exe]) => [Number(pid), exe ?? ""]),
  );
}

/** An app started from its executable, as LaunchServices would, with no switches. */
export class App {
  readonly pid: number;
  readonly #log: string;
  readonly #from: number;

  constructor(
    readonly path: string,
    readonly name: string,
  ) {
    this.#log = join(logs(name), "main.log");
    this.#from = size(this.#log);
    const exe = String(plist(join(path, "Contents/Info.plist")).CFBundleExecutable);
    const pid = spawn(join(path, "Contents/MacOS", exe), [], { stdio: "ignore" }).pid;
    if (pid === undefined) {
      throw new Error(`${path} did not start`);
    }
    this.pid = pid;
  }

  /**
   * Once the window's connection is up. The control connection came first,
   * through the native extension (design 5.5.1); a failure or an error in
   * `main.log` ends the wait.
   */
  async ready(): Promise<void> {
    await until(`${this.name} ready`, () => {
      const lines = linesSince(this.#log, this.#from);
      const failed = lines.find((line) => line.level === "error" || line.status === "failed");
      if (failed) {
        throw new Error(`${this.name}: ${JSON.stringify(failed)}`);
      }
      const ready = lines.some((line) => line.msg === "host connection" && line.status === "ready");
      return (ready && lines.some((line) => line.msg === "window connected")) || undefined;
    });
  }

  /** The host in `host.lock`, as it runs: its pid, executable and cdhash. */
  host() {
    const pid = Number(readFileSync(join(data(this.name), "host.lock"), "utf8").trim());
    const { details } = codesign(pid);
    return { pid, exe: field(details, "Executable"), cdhash: field(details, "CDHash") };
  }

  /** What runs from inside the app, as paths in it. */
  running(): string[] {
    const inside = [...processes().values()].filter((exe) => exe.startsWith(`${this.path}/`));
    return inside.map((exe) => exe.slice(this.path.length + 1)).sort();
  }

  /** SIGTERM, which Electron takes as ⌘Q; then nothing may run from the app. */
  async quit(): Promise<void> {
    if (processes().get(this.pid)?.startsWith(`${this.path}/`)) {
      process.kill(this.pid, "SIGTERM");
    }
    await until(`${this.name} gone`, () => this.running().length === 0 || undefined, 30_000);
  }
}
