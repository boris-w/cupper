// The CLI as the tests' second client (design 5.9.4): `--json` output (T046's
// exit codes and markers), as `test_harness` with the key on fd 3, or as the
// user's plain `cli` client, which needs `elevate` to write. Every process it
// starts is ended by `dispose`.

import { spawn, type ChildProcess } from "node:child_process";
import { closeSync, openSync } from "node:fs";
import { createInterface } from "node:readline";

import { BUILT } from "./paths";

export type Line = Record<string, unknown>;

/** As whom the CLI connects: the test's `harness` key, or the user's `cli`. */
export type As = "harness" | "cli";

export interface Result {
  code: number;
  lines: Line[];
  stderr: string;
}

/** A running CLI's output lines, as they come. */
export class Output {
  readonly lines: Line[] = [];
  stderr = "";
  readonly #waiters = new Set<() => void>();
  readonly #child: ChildProcess;
  readonly closed: Promise<number>;

  constructor(child: ChildProcess) {
    this.#child = child;
    createInterface({ input: piped(child.stdout) }).on("line", (text) => {
      this.lines.push(JSON.parse(text) as Line);
      this.#wake();
    });
    piped(child.stderr).on("data", (chunk: Buffer) => (this.stderr += chunk.toString()));
    this.closed = new Promise((done) =>
      child.once("close", (status, signal) => {
        done(status ?? (signal ? 128 : 1));
        this.#wake();
      }),
    );
  }

  /** The first line from `from` on that `test` accepts, and its index. */
  async until(test: (line: Line) => boolean, from = 0, ms = 20_000): Promise<[Line, number]> {
    const deadline = Date.now() + ms;
    let ended = false;
    void this.closed.then(() => (ended = true));
    for (;;) {
      for (const [at, line] of this.lines.entries()) {
        if (at >= from && test(line)) {
          return [line, at];
        }
      }
      if (ended || Date.now() >= deadline) {
        const why = ended ? "the CLI ended" : `none in ${ms} ms`;
        throw new Error(`cli: no such line (${why}); stderr: ${this.stderr}`);
      }
      await new Promise<void>((wake) => {
        this.#waiters.add(wake);
        setTimeout(wake, Math.min(deadline - Date.now(), 200));
      });
    }
  }

  /** Ends the CLI; `dispose` waits for it. */
  stop(): void {
    this.#child.kill("SIGTERM");
  }

  #wake() {
    this.#waiters.forEach((wake) => wake());
    this.#waiters.clear();
  }
}

/** `repl`: commands on one connection, each ending in `{"repl": {"exit": n}}`. */
export class Repl {
  readonly #child: ChildProcess;
  readonly output: Output;
  #read = 0;

  constructor(child: ChildProcess) {
    this.#child = child;
    this.output = new Output(child);
  }

  /** One command; its exit code and the lines before its end marker. */
  async run(line: string, ms = 20_000): Promise<Result> {
    piped(this.#child.stdin).write(`${line}\n`);
    const [end, at] = await this.output.until((out) => "repl" in out, this.#read, ms);
    const lines = this.output.lines.slice(this.#read, at);
    this.#read = at + 1;
    return { code: (end.repl as { exit: number }).exit, lines, stderr: this.output.stderr };
  }
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
  async run(args: string[], as: As = "harness"): Promise<Result> {
    const output = new Output(this.#start(args, as));
    const code = await output.closed;
    return { code, lines: output.lines, stderr: output.stderr };
  }

  /** One command that must succeed; its first line. */
  async ok(args: string[], as: As = "harness"): Promise<Line> {
    const { code, lines, stderr } = await this.run(args, as);
    const [first] = lines;
    if (code !== 0 || first === undefined) {
      throw new Error(`cupper ${args.join(" ")}: exit ${code}; ${stderr}`);
    }
    return first;
  }

  /** A command that keeps running (`watch`, `sessions --watch`), and its output. */
  watch(args: string[], as: As = "harness"): Output {
    return new Output(this.#start(args, as));
  }

  repl(as: As = "harness"): Repl {
    return new Repl(this.#start(["repl"], as));
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

  #start(args: string[], as: As): ChildProcess {
    const key = as === "harness" ? openSync(this.#keyFile, "r") : "ignore";
    const harness = typeof key === "number" ? ["--harness-key-fd", "3"] : [];
    const options = ["--data-dir", this.#dataDir, "--json", ...harness];
    const child = spawn(BUILT.cli, [...options, ...args], {
      stdio: ["pipe", "pipe", "pipe", key],
    });
    if (typeof key === "number") {
      closeSync(key);
    }
    this.#children.add(child);
    child.once("close", () => this.#children.delete(child));
    return child;
  }
}

function piped<T>(stream: T | null): T {
  if (stream === null) {
    throw new Error("the CLI was started without this pipe");
  }
  return stream;
}
