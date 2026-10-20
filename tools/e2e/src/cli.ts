// The CLI as the tests' second client (design 5.9.4): `test_harness`, with
// the key on fd 3, and `--json` output (T046's exit codes). Every process it
// starts is ended by `dispose`.

import { spawn, type ChildProcess } from "node:child_process";
import { closeSync, openSync } from "node:fs";

import { BUILT } from "./paths";

export type Line = Record<string, unknown>;

export interface Result {
  code: number;
  lines: Line[];
  stderr: string;
}

export class Cli {
  readonly #dataDir: string;
  readonly #keyFile: string;
  readonly #children = new Set<ChildProcess>();

  constructor(dataDir: string, keyFile: string) {
    this.#dataDir = dataDir;
    this.#keyFile = keyFile;
  }

  /** One command; its exit code and output. */
  async run(args: string[]): Promise<Result> {
    const key = openSync(this.#keyFile, "r");
    const options = ["--data-dir", this.#dataDir, "--json", "--harness-key-fd", "3"];
    const child = spawn(BUILT.cli, [...options, ...args], {
      stdio: ["ignore", "pipe", "pipe", key],
    });
    closeSync(key);
    this.#children.add(child);
    let [stdout, stderr] = ["", ""];
    child.stdout?.on("data", (chunk: Buffer) => (stdout += chunk.toString()));
    child.stderr?.on("data", (chunk: Buffer) => (stderr += chunk.toString()));
    const code = await new Promise<number>((done) =>
      child.once("close", (status, signal) => done(status ?? (signal ? 128 : 1))),
    );
    this.#children.delete(child);
    const lines = stdout
      .split("\n")
      .filter(Boolean)
      .map((text) => JSON.parse(text) as Line);
    return { code, lines, stderr };
  }

  /** One command that must succeed; its first line. */
  async ok(args: string[]): Promise<Line> {
    const { code, lines, stderr } = await this.run(args);
    const [first] = lines;
    if (code !== 0 || first === undefined) {
      throw new Error(`cupper ${args.join(" ")}: exit ${code}; ${stderr}`);
    }
    return first;
  }

  /** Ends every CLI still running: SIGTERM, then SIGKILL after 3 s. */
  async dispose(): Promise<void> {
    await Promise.all(
      [...this.#children].map(async (child) => {
        const ended = new Promise((done) => child.once("close", done));
        child.kill("SIGTERM");
        const timer = setTimeout(() => child.kill("SIGKILL"), 3000);
        await ended;
        clearTimeout(timer);
      }),
    );
  }
}
