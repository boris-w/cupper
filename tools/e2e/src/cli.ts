// The CLI as the tests' second client (design 5.9.4): `--json` output, as
// `test_harness` with the key on fd 3 or as a plain `cli` client. Every
// process it starts is ended by `dispose` (T046's exit codes and markers).

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

/** A running CLI's output lines, read as they come. */
export class Output {
  readonly lines: Line[] = [];
  readonly #waiters = new Set<() => void>();
  stderr = "";
  #ended = false;

  constructor(child: ChildProcess) {
    createInterface({ input: piped(child.stdout) }).on("line", (text) => {
      this.lines.push(JSON.parse(text) as Line);
      this.#wake();
    });
    piped(child.stderr).on("data", (chunk: Buffer) => (this.stderr += chunk.toString()));
    child.once("close", () => {
      this.#ended = true;
      this.#wake();
    });
  }

  /** The first line from `from` on that `test` accepts, and its index. */
  async until(test: (line: Line) => boolean, from = 0, ms = 10_000): Promise<[Line, number]> {
    const deadline = Date.now() + ms;
    for (;;) {
      for (const [at, line] of this.lines.entries()) {
        if (at >= from && test(line)) {
          return [line, at];
        }
      }
      if (this.#ended || Date.now() >= deadline) {
        const why = this.#ended ? "the CLI ended" : `none in ${ms} ms`;
        throw new Error(`cli: no such line (${why}); stderr: ${this.stderr}`);
      }
      await new Promise<void>((wake) => {
        this.#waiters.add(wake);
        setTimeout(wake, deadline - Date.now());
      });
    }
  }

  #wake() {
    this.#waiters.forEach((wake) => wake());
    this.#waiters.clear();
  }
}

/** `repl`: one command at a time on one connection, each ending in `{"repl": {"exit": n}}`. */
export class Repl {
  readonly #child: ChildProcess;
  readonly output: Output;
  #read = 0;

  constructor(child: ChildProcess) {
    this.#child = child;
    this.output = new Output(child);
  }

  async run(line: string, ms = 10_000): Promise<Result> {
    piped(this.#child.stdin).write(`${line}\n`);
    const isExit = (out: Line) => typeof out.repl === "object";
    const [end, at] = await this.output.until(isExit, this.#read, ms);
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
    const child = this.start(args, as);
    const output = new Output(child);
    const code = await new Promise<number>((done) =>
      child.once("close", (status, signal) => done(status ?? (signal ? 128 : 1))),
    );
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

  /** A command that keeps running (`watch`, `sessions --watch`) and its output. */
  watch(args: string[], as: As = "harness"): Output {
    return new Output(this.start(args, as));
  }

  repl(as: As = "harness"): Repl {
    return new Repl(this.start(["repl"], as));
  }

  /** Ends every CLI still running: SIGTERM, then SIGKILL after 3 s. */
  async dispose(): Promise<void> {
    const ended = (child: ChildProcess) =>
      child.exitCode !== null || child.signalCode !== null
        ? Promise.resolve()
        : new Promise<void>((done) => child.once("close", () => done()));
    await Promise.all(
      [...this.#children].map(async (child) => {
        child.kill("SIGTERM");
        const timer = setTimeout(() => child.kill("SIGKILL"), 3000);
        await ended(child);
        clearTimeout(timer);
      }),
    );
  }

  start(args: string[], as: As): ChildProcess {
    const harness = as === "harness";
    const key = harness ? openSync(this.#keyFile, "r") : "ignore";
    const options = ["--data-dir", this.#dataDir, "--json"];
    const child = spawn(
      BUILT.cli,
      [...options, ...(harness ? ["--harness-key-fd", "3"] : []), ...args],
      { stdio: ["pipe", "pipe", "pipe", key] },
    );
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
