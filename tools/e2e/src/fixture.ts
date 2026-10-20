// The e2e fixture (design 5.9.4, 5.9.9): each test gets a temporary root
// with the data directory, userData, the logs, the `harness` key and the
// host's `test_config` (the layout of `CUPPER_E2E_DIR`, T074), a temporary
// git repository and the fake agent. Only the app starts the host; the CLI
// joins as a second client. At the end the app quits, the host is stopped,
// and every process the test started must be gone.

import { execFileSync, type ChildProcess } from "node:child_process";
import { randomBytes } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import {
  _electron,
  test as base,
  expect,
  type ElectronApplication,
  type Page,
  type TestInfo,
} from "@playwright/test";

import { Cli, type Line } from "./cli";
import { Dialogs } from "./dialogs";
import { BUILT, ROOTS } from "./paths";
import { alive, gone, processes, signal, tree, type Proc } from "./procs";

/** Who made a root: a Playwright worker. A root whose owner is gone is stale. */
const OWNER = "owner.json";
/** The processes a root's test started, for cleaning up after a killed worker. */
const PROCS = "procs.json";

export class Cupper {
  readonly root: string;
  readonly dataDir: string;
  /** A git repository with one commit, for projects. */
  readonly repo: string;
  readonly cli: Cli;
  #app: ElectronApplication | undefined;
  /** The app's main process; `app.process()` throws once the app is closed. */
  #main: ChildProcess | undefined;
  #window: Page | undefined;
  #dialogs: Dialogs | undefined;
  #procs: Proc[] = [];

  constructor(testConfig: Record<string, unknown>) {
    mkdirSync(ROOTS, { recursive: true, mode: 0o700 });
    this.root = realpathSync(mkdtempSync(join(ROOTS, "t-")));
    const owner = processes().filter((proc) => proc.pid === process.pid);
    writeFileSync(join(this.root, OWNER), JSON.stringify(owner));
    this.dataDir = join(this.root, "data");
    const key = join(this.root, "harness_key");
    writeFileSync(key, `${randomBytes(32).toString("hex")}\n`, { mode: 0o600 });
    const config = {
      agents: { claude: { program: BUILT.fakeAgent } },
      login_env: {},
      notifier: "record",
      ...testConfig,
    };
    writeFileSync(join(this.root, "test_config.json"), JSON.stringify(config));
    this.repo = gitRepo(join(this.root, "repo"));
    this.cli = new Cli(this.dataDir, key);
  }

  get app(): ElectronApplication {
    return required(this.#app);
  }

  get window(): Page {
    return required(this.#window);
  }

  get dialogs(): Dialogs {
    return required(this.#dialogs);
  }

  /** Starts the app, replaces its native dialogs, and waits for its window. */
  async launch(): Promise<void> {
    const env = { ...process.env, CUPPER_E2E_DIR: this.root };
    this.#app = await _electron.launch({ executablePath: BUILT.app, env });
    this.#main = this.#app.process();
    if (this.#main.pid === undefined) {
      throw new Error("the app did not start");
    }
    this.#record(tree(this.#main.pid));
    this.#dialogs = await Dialogs.install(this.app);
    this.#window = await this.app.firstWindow();
  }

  /** `host/status` once the window's data connection is up; the host's pid recorded. */
  async connected(): Promise<Line> {
    let status: Line = {};
    const window = (client: Line) =>
      client.principal_kind === "desktop_ui" && client.role === "window";
    await expect
      .poll(
        async () => {
          const { code, lines } = await this.cli.run(["status"]);
          status = lines[0] ?? {};
          return code === 0 && ((status.clients ?? []) as Line[]).some(window);
        },
        { message: "the window connects to the host", timeout: 20_000 },
      )
      .toBe(true);
    if (typeof status.pid !== "number") {
      throw new Error("the host gives no pid: not a test-harness build");
    }
    this.#record(tree(status.pid));
    return status;
  }

  /** Quits as on shutdown: the app does not ask about running turns (design 5.7.1). */
  async quitWithoutAsking(): Promise<void> {
    await this.app.evaluate(({ powerMonitor }) => powerMonitor.emit("shutdown"));
    await this.app.close();
  }

  /**
   * Quits the app and stops the host, then fails if any process the test
   * started is still there; those are killed first.
   */
  async dispose(info: TestInfo): Promise<void> {
    const problems: string[] = [];
    await this.cli.dispose();
    const table = processes();
    const app = this.#main;
    const host = this.#host(table);
    this.#record([
      ...(app?.pid !== undefined ? tree(app.pid, table) : []),
      ...(host ? tree(host.pid, table) : []),
    ]);
    if (app && app.exitCode === null && app.signalCode === null) {
      await this.quitWithoutAsking().catch(() => problems.push("the app did not quit"));
    }
    if (host && (await gone([host], 0)).length > 0) {
      signal([host], "SIGTERM");
      // An orderly exit closes the adapters first (design 5.2.13).
      if ((await gone([host], 10_000)).length > 0) {
        problems.push(`the host (${host.pid}) did not exit on SIGTERM`);
      }
    }
    const left = await gone(this.#procs, 5000);
    if (left.length > 0) {
      signal(left, "SIGKILL");
      problems.push(`left behind: ${left.map((proc) => `${proc.pid} ${proc.args}`).join(", ")}`);
    }
    if (info.status !== info.expectedStatus) {
      await attachLogs(info, this.root);
    }
    rmSync(this.root, { recursive: true, force: true });
    if (problems.length > 0) {
      throw new Error(problems.join("; "));
    }
  }

  /** The host in `host.lock`, if it is this test's: it runs with our data directory. */
  #host(table: Proc[]): Proc | undefined {
    let pid: number;
    try {
      pid = Number(readFileSync(join(this.dataDir, "host.lock"), "utf8").trim());
    } catch {
      return undefined;
    }
    return table.find(
      (proc) => proc.pid === pid && proc.args === `${BUILT.host} --data-dir ${this.dataDir}`,
    );
  }

  #record(procs: Proc[]) {
    const known = (proc: Proc) =>
      this.#procs.some((p) => p.pid === proc.pid && p.started === proc.started);
    this.#procs.push(...procs.filter((proc) => !known(proc)));
    writeFileSync(join(this.root, PROCS), JSON.stringify(this.#procs));
  }
}

function required<T>(value: T | undefined): T {
  if (value === undefined) {
    throw new Error("the app has not been launched");
  }
  return value;
}

function gitRepo(dir: string): string {
  mkdirSync(dir);
  const git = (...args: string[]) =>
    execFileSync("git", ["-C", dir, "-c", "commit.gpgsign=false", ...args], {
      env: {
        PATH: process.env.PATH,
        HOME: dir,
        GIT_CONFIG_NOSYSTEM: "1",
        GIT_AUTHOR_NAME: "cupper-e2e",
        GIT_AUTHOR_EMAIL: "e2e@cupper.invalid",
        GIT_COMMITTER_NAME: "cupper-e2e",
        GIT_COMMITTER_EMAIL: "e2e@cupper.invalid",
      },
    });
  git("init", "--quiet", "--initial-branch=main");
  writeFileSync(join(dir, "README.md"), "e2e\n");
  git("add", "README.md");
  git("commit", "--quiet", "--message", "init");
  return dir;
}

async function attachLogs(info: TestInfo, root: string) {
  for (const dir of ["logs", "data/logs"]) {
    try {
      const files = execFileSync("find", [join(root, dir), "-type", "f"], { encoding: "utf8" });
      for (const path of files.split("\n").filter(Boolean)) {
        await info.attach(path.slice(root.length + 1), { path });
      }
    } catch {
      // No such directory.
    }
  }
}

/**
 * Removes the roots of tests whose worker was killed, and the processes
 * those tests left: the ones they recorded, any that names the root, and
 * the processes below them.
 */
export async function sweep(roots: string[]): Promise<void> {
  const table = processes();
  for (const root of roots) {
    const read = (name: string): Proc[] => {
      try {
        return JSON.parse(readFileSync(join(root, name), "utf8")) as Proc[];
      } catch {
        return [];
      }
    };
    if (alive(read(OWNER), table).length > 0) {
      continue;
    }
    const found = [...alive(read(PROCS), table), ...table.filter((p) => p.args.includes(root))];
    const left = found.flatMap((proc) => tree(proc.pid, table));
    signal(left, "SIGTERM");
    signal(await gone(left, 10_000), "SIGKILL");
    rmSync(root, { recursive: true, force: true });
  }
}

export const test = base.extend<{ testConfig: Record<string, unknown>; cupper: Cupper }>({
  /** The host's `test_config` beyond the fake agent: `clock`, `durations`, failpoints. */
  testConfig: [{}, { option: true }],
  cupper: async ({ testConfig }, use, info) => {
    const cupper = new Cupper(testConfig);
    try {
      await use(cupper);
    } finally {
      await cupper.dispose(info);
    }
  },
});

export { expect };
