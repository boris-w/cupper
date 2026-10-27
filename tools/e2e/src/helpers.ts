// The e2e helpers on top of the fixture (design 5.9.4, 5.9.9): the window's
// host client, dropping a window's connection, the barriers, killing the
// host, the CLI's elevation, and sampling the network.

import { execFileSync } from "node:child_process";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";

import { createI18n } from "@cupper/i18n";

import type { Line, Repl } from "./cli";
import { expect, test, type Cupper } from "./fixture";
import { gone, signal, tree } from "./procs";

declare global {
  interface Window {
    /** e2e builds only (`apps/desktop/src/renderer/main.ts`). */
    __cupperTest: {
      counters(): { gaps: number; duplicates: number; resets: number };
      state(): { status: string };
      /** The streams the window follows: `{seq, complete, view}` by stream. */
      views(): Record<string, unknown>;
      /** A command sent as the window: `{result}`, or `{error}` from the bridge or the host. */
      command(method: string, params: Record<string, unknown>): Promise<Record<string, unknown>>;
    };
  }
}

/** The app's text in the app's language, as its dialogs and sidebar show it. */
export async function texts(cupper: Cupper) {
  const locale = await cupper.app.evaluate(({ app }) => app.getPreferredSystemLanguages()[0]);
  return createI18n(locale).t;
}

/** The window's host client: sync counters and connection state (`window.__cupperTest`). */
export const hostClient = (cupper: Cupper) => ({
  counters: () => cupper.window.evaluate(() => window.__cupperTest.counters()),
  state: () => cupper.window.evaluate(() => window.__cupperTest.state()),
});

/** The connections of the app's windows to the host, by `host/status`. */
async function windowConnections(cupper: Cupper): Promise<unknown[]> {
  const status = await cupper.cli.ok(["status"]);
  return (status.clients as Line[])
    .filter((client) => client.principal_kind === "desktop_ui" && client.role === "window")
    .map((client) => client.conn_id);
}

/**
 * Drops the window's data connection as a host would, without a reload
 * (the main process's e2e hook, E2E-NFR1-4); returns once it is back.
 */
export async function disconnectWindow(cupper: Cupper): Promise<void> {
  const before = await windowConnections(cupper);
  const window = await cupper.app.browserWindow(cupper.window);
  const id = await window.evaluate((w) => w.webContents.id);
  await cupper.app.evaluate((_electron, id) => {
    const hooks = (globalThis as { __cupperE2E?: { disconnectWindow(id: number): void } })
      .__cupperE2E;
    hooks?.disconnectWindow(id);
  }, id);
  test.info().annotations.push({ type: "disconnected", description: new Date().toISOString() });
  const started = Date.now();
  try {
    await expect
      .poll(async () => (await windowConnections(cupper)).some((conn) => !before.includes(conn)), {
        message: "the window connects again",
      })
      .toBe(true);
  } catch (error) {
    // What the window says and whether the main process answers; the
    // fixture attaches the logs (T199).
    const state = await hostClient(cupper)
      .state()
      .catch((e: unknown) => String(e));
    const main = await Promise.race([
      cupper.app.evaluate(() => "responds"),
      new Promise<string>((done) => setTimeout(() => done("no answer in 5 s"), 5000)),
    ]).catch((e: unknown) => String(e));
    const body = JSON.stringify({ waited_ms: Date.now() - started, window: state, main });
    await test.info().attach("reconnect", { body, contentType: "application/json" });
    throw error;
  }
  test.info().annotations.push({ type: "reconnect_ms", description: String(Date.now() - started) });
}

/**
 * `test/clock_advance` on the manual clock: it returns once the timers due
 * have fired and their writes and events are out (design 5.2.16).
 */
export const advanceClock = (cupper: Cupper, ms: number) =>
  cupper.cli.ok(["clock-advance", String(ms)]);

export interface StoredEvent {
  seq: number;
  type: string;
  data: Record<string, unknown>;
}

/** The events of `stream` (`index`, `session/<id>`) the host has written so far. */
export function events(cupper: Cupper, stream: string): StoredEvent[] {
  const db = new DatabaseSync(join(cupper.dataDir, "host.db"), { readOnly: true });
  try {
    const rows = db
      .prepare("SELECT seq, type, json FROM events WHERE stream = ? ORDER BY seq")
      .all(stream) as { seq: number; type: string; json: string }[];
    const data = (json: string) => (JSON.parse(json) as { data?: Record<string, unknown> }).data;
    return rows.map(({ seq, type, json }) => ({ seq, type, data: data(json) ?? {} }));
  } finally {
    db.close();
  }
}

/**
 * The first event of `stream` that `test` accepts, once the host has written
 * it: a barrier on an agent or the host, whether it came already or not.
 */
export async function untilEvent(
  cupper: Cupper,
  stream: string,
  test: (event: StoredEvent) => boolean,
): Promise<StoredEvent> {
  let found: StoredEvent | undefined;
  await expect
    .poll(() => (found = events(cupper, stream).find(test)), { message: `an event of ${stream}` })
    .toBeDefined();
  return found as StoredEvent;
}

/**
 * The session as the index lists it, once it does. On the manual clock the
 * index writes after 100 ms: advance the clock past that first (T046).
 */
export async function untilSession(cupper: Cupper, session: string): Promise<Line> {
  const listed = cupper.cli.watch(["sessions", "--watch"]);
  try {
    const [line] = await listed.until((line) => line.session_id === session);
    return line;
  } finally {
    listed.stop();
  }
}

/**
 * SIGKILLs this test's host, at a moment the test chose; returns the new
 * host's status once the app has started it and its window is connected.
 */
export async function killHost(cupper: Cupper): Promise<Line> {
  const host = cupper.host();
  if (!host) {
    throw new Error("no host of this test runs");
  }
  // Its adapters must not outlive it either.
  cupper.track(tree(host.pid));
  signal([host], "SIGKILL");
  expect(await gone([host], 5000)).toEqual([]);
  const status = await cupper.connected();
  expect(status.pid).not.toBe(host.pid);
  return status;
}

/**
 * The user's way to write from the CLI: a `cli` client's `repl` asks to
 * elevate, the sidebar's banner opens the native dialog, and the stand-in
 * approves, with the approve box ticked when it is asked for. The `repl`,
 * now allowed `scopes`.
 */
export async function elevate(cupper: Cupper, scopes = ["write", "approve"]): Promise<Repl> {
  const t = await texts(cupper);
  const repl = cupper.cli.repl("cli");
  const dialogs = (await cupper.dialogs.calls()).length;
  const asked = repl.run(`elevate --scopes ${scopes.join(",")}`, 30_000);
  await cupper.window.getByRole("button", { name: t("sidebar:review") }).click();
  const dialog = await cupper.dialogs.next(dialogs);
  await cupper.dialogs.answer(dialog.index, 0, scopes.includes("approve"));
  const { code, lines, stderr } = await asked;
  expect(code, stderr).toBe(0);
  expect(lines[0]?.granted).toEqual(scopes);
  return repl;
}

export interface Flow {
  pid: number;
  /** `tcp4`, `udp6`, ... */
  protocol: string;
  local: string;
  remote: string;
  state: string;
}

/**
 * One `nettop` sample of this test's processes, the app's and the host's
 * with its adapters, and their TCP and UDP flows. A sampler only: whether
 * `nettop` can carry the egress acceptance is T092's question (Q27).
 */
export function sampleNetwork(cupper: Cupper): Flow[] {
  const pids = cupper.processes().flatMap((proc) => ["-p", String(proc.pid)]);
  const out = execFileSync("nettop", ["-L", "1", "-n", "-x", ...pids], { encoding: "utf8" });
  const flows: Flow[] = [];
  let pid: number | undefined;
  // `<time>,<name>.<pid>,...` starts a process; its flows follow:
  // `<time>,<protocol> <local><-><remote>,<interface>,<state>,...`.
  for (const [, what = "", , state = ""] of out.split("\n").map((line) => line.split(","))) {
    const flow = /^(\w+) (.+)<->(.+)$/.exec(what);
    if (flow) {
      const [, protocol = "", local = "", remote = ""] = flow;
      if (pid !== undefined) flows.push({ pid, protocol, local, remote, state });
    } else {
      const owner = /\.(\d+)$/.exec(what);
      pid = owner ? Number(owner[1]) : undefined;
    }
  }
  return flows;
}

/** Those of `flows` whose far end is not this machine. */
export function offMachine(flows: Flow[]): Flow[] {
  return flows.filter((flow) => !/^(127\.|::1[.:]|\*[.:])/.test(flow.remote));
}
