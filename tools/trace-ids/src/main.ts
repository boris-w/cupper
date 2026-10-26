/**
 * `trace-ids`: the test ID registry `tests/ids.toml` against the docs, the test
 * results and the evidence of manual checks (design 5.9.10, plan 1.6).
 *
 *     node tools/trace-ids/src/main.ts <command> [options]
 */
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { complete, evaluate, select, type State } from "./evaluate.ts";
import { checkRegistry, MILESTONES, oneOf, parseRegistry, type Entry } from "./registry.ts";
import { readCompat, readJunit, readPlaywright, readSummary, type Result } from "./results.ts";

const USAGE = `usage: trace-ids <command> [options]

commands:
  check   check tests/ids.toml against the docs (run by \`xtask ci testdata-scan\`)
  status  list the done, settled, pending and problem IDs
  verify  fail unless every selected ID is done or settled:
            --milestone <M>  the IDs due by M (${MILESTONES.join(", ")}); results
                             from committed summaries of a single commit only
            --task <Txxx>    the IDs a task produces

results, for status and verify (repeatable):
  --summary <file>             a summary written by \`xtask ci --record\`
  --junit <suite>=<file>       a cargo-nextest JUnit report of that suite
  --playwright <suite>=<file>  a Playwright JSON report of that suite
  --compat <file>              a report of \`cargo xtask compat\` with a real CLI, for the
                               COMPAT IDs of its CLI and scenarios (\`scenario\`, \`cli\`)`;

const root = resolve(import.meta.dirname, "../../..");

class UsageError extends Error {}

interface Options {
  results: Result[];
  /** Compatibility scenarios with no COMPAT ID. */
  unproven: string[];
  milestone?: (typeof MILESTONES)[number];
  task?: string;
}

function main([command, ...args]: string[]): number {
  if (command !== "check" && command !== "status" && command !== "verify") {
    throw new UsageError(command ? `unknown command \`${command}\`` : "no command");
  }
  const { registry, problems } = parseRegistry(read("tests/ids.toml"));
  if (command === "check" && problems.length === 0) {
    problems.push(...checkRegistry(registry, docs(), (path) => existsSync(join(root, path))));
  }
  if (problems.length) {
    problems.forEach((problem) => console.error(`tests/ids.toml: ${problem}`));
    return 1;
  }
  if (command === "check") {
    console.log(`tests/ids.toml: ${registry.entries.size} IDs, consistent with the docs`);
    return 0;
  }

  const options = parse(args, registry.entries);
  const versions = { electron: electron() };
  const { states, stray } = evaluate(registry, options.results, { today: today(), versions });
  const rows = (entries: Entry[], keep: (state: State) => boolean) =>
    entries.flatMap((entry) => {
      const state = states.get(entry.id);
      return state && keep(state) ? [{ entry, state }] : [];
    });
  if (command === "status") {
    const all = [...registry.entries.values()];
    for (const standing of ["done", "settled", "pending", "problem"]) {
      print(
        standing,
        rows(all, (state) => state.standing === standing),
      );
    }
    printStray(stray);
    printUnproven(options.unproven);
    return 0;
  }
  if (!options.milestone && !options.task) {
    throw new UsageError("verify needs --milestone or --task");
  }
  const selected = select(registry, options);
  const open = rows(selected, (state) => !complete(state));
  print("not done", open);
  printStray(stray);
  printUnproven(options.unproven);
  const scope = [options.milestone, options.task].filter(Boolean).join(", ");
  console.log(`${scope}: ${open.length} of ${selected.length} IDs not done`);
  return open.length || stray.length ? 1 : 0;
}

function parse(args: string[], entries: Map<string, Entry>): Options {
  const options: Options = { results: [], unproven: [] };
  const ids = [...entries.keys()];
  const summaries: string[] = [];
  const reports: string[] = [];
  const commits = new Set<string>();
  let raw = false;
  for (let i = 0; i < args.length; i += 2) {
    const [flag, value] = [args[i], args[i + 1]];
    if (value === undefined) {
      throw new UsageError(`\`${flag}\` needs a value`);
    }
    const [suite, file] = value.split(/=(.*)/s);
    if (flag === "--summary") {
      const summary = readSummary(readFileSync(value, "utf8"), value);
      commits.add(summary.commit);
      summaries.push(value);
      options.results.push(...summary.results);
    } else if (flag === "--compat") {
      const compat = readCompat(readFileSync(value, "utf8"), value, entries.values());
      reports.push(value);
      options.results.push(...compat.results);
      options.unproven.push(...compat.unproven);
    } else if ((flag === "--junit" || flag === "--playwright") && suite && file) {
      const reader = flag === "--junit" ? readJunit : readPlaywright;
      options.results.push(...reader(readFileSync(file, "utf8"), suite, ids, file));
      raw = true;
    } else if (flag === "--milestone" && oneOf(MILESTONES, value)) {
      options.milestone = value;
    } else if (flag === "--task" && /^T\d{3}$/.test(value)) {
      options.task = value;
    } else {
      throw new UsageError(`bad option \`${flag} ${value}\``);
    }
  }
  const unchanged = [...summaries, ...reports].every(committed);
  if (options.milestone && (raw || commits.size !== 1 || !unchanged)) {
    throw new UsageError(
      "--milestone reads committed summaries of a single commit, and committed compatibility reports, only",
    );
  }
  return options;
}

/** Tracked and unchanged: a milestone exit can be checked again from the repository. */
function committed(file: string): boolean {
  try {
    execFileSync("git", ["ls-files", "--error-unmatch", "--", file], { stdio: "ignore" });
    return execFileSync("git", ["status", "--porcelain", "--", file], { encoding: "utf8" }) === "";
  } catch {
    return false;
  }
}

function print(heading: string, rows: { entry: Entry; state: State }[]): void {
  const order = (entry: Entry) => MILESTONES.indexOf(entry.milestone);
  rows.sort(
    (a, b) =>
      order(a.entry) - order(b.entry) ||
      (a.entry.task ?? "~").localeCompare(b.entry.task ?? "~") ||
      a.entry.id.localeCompare(b.entry.id),
  );
  console.log(`${heading} (${rows.length})`);
  const width = Math.max(0, ...rows.map((row) => row.entry.id.length));
  for (const { entry, state } of rows) {
    const where = `${entry.milestone} ${entry.task ?? "-"}`.padEnd(12);
    console.log(`  ${entry.id.padEnd(width)}  ${where}${state.detail}`);
  }
}

/** The scenarios of the compatibility reports that prove no ID: most of the basic ones. */
function printUnproven(unproven: string[]): void {
  if (unproven.length) {
    console.log(`compatibility scenarios without a COMPAT ID (${unproven.length}), not judged`);
    unproven.forEach((scenario) => console.log(`  ${scenario}`));
  }
}

function printStray(stray: Result[]): void {
  if (stray.length) {
    console.log(`unregistered (${stray.length})`);
    stray.forEach((result) => console.log(`  ${result.id}  ${result.source}`));
  }
}

function read(path: string): string {
  return readFileSync(join(root, path), "utf8");
}

/** The plan and the design, which define the test IDs. */
function docs(): Map<string, string> {
  const design = readdirSync(join(root, "docs/design")).sort();
  const files = ["docs/03-plan.md", ...design.map((name) => `docs/design/${name}`)];
  return new Map(files.map((file) => [file, read(file)]));
}

function today(): string {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

/** The Electron version the desktop app is built with. */
function electron(): string | undefined {
  const manifest = JSON.parse(read("apps/desktop/package.json"));
  return manifest.devDependencies?.electron ?? manifest.dependencies?.electron;
}

try {
  process.exitCode = main(process.argv.slice(2));
} catch (err) {
  console.error(`trace-ids: ${(err as Error).message}`);
  if (err instanceof UsageError) {
    console.error(`\n${USAGE}`);
  }
  process.exitCode = 2;
}
