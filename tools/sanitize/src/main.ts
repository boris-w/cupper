/**
 * `sanitize`: redacts recordings before they enter `testdata/` and scans test
 * data for personal data (design 5.9.3). Findings name the file, the line and
 * the rule, never the matching text.
 *
 *     node tools/sanitize/src/main.ts <command> [options]
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { currentMachine, sanitizeTree } from "./sanitize.ts";
import { expand, patterns, report, scan, sensitiveWords, wordList, type Finding } from "./scan.ts";

const USAGE = `usage: sanitize <command> [options]

commands:
  sanitize --in <dir> --out <dir> [--base <dir>]... [--terms <file>]
          redact every .jsonl under --in into the same path under --out, then
          scan the output; --base names a temporary root of the recorded run,
          --terms a local word list (one per line, # starts a comment)
  scan <path or glob>...
          generic patterns: home and temporary paths, email addresses, tokens
  words --list <file> <path or glob>...
          the entries of a local sensitive word list`;

class UsageError extends Error {}

/** Exit code 0: clean; 1: something was found; 2: bad usage or an error. */
export function main([command, ...args]: string[], cwd: string, log = console.log): number {
  const flags = new Map<string, string[]>();
  const targets: string[] = [];
  for (let i = 0; i < args.length; i++) {
    const arg = args[i] ?? "";
    if (!arg.startsWith("--")) {
      targets.push(arg);
      continue;
    }
    const value = args[++i];
    if (value === undefined) throw new UsageError(`\`${arg}\` needs a value`);
    flags.set(arg, [...(flags.get(arg) ?? []), value]);
  }
  /** The path given once with `flag`, relative to `cwd`. */
  const one = (flag: string) => {
    const values = flags.get(flag) ?? [];
    if (values.length !== 1) throw new UsageError(`\`${flag}\` is needed once`);
    return resolve(cwd, values[0] ?? "");
  };
  const known = (...allowed: string[]) => {
    const unknown = [...flags.keys()].find((flag) => !allowed.includes(flag));
    if (unknown) throw new UsageError(`unknown option \`${unknown}\``);
  };

  let findings: Finding[];
  let files: string[];
  if (command === "sanitize" && targets.length === 0) {
    known("--in", "--out", "--base", "--terms");
    const terms = flags.has("--terms") ? wordList(readFileSync(one("--terms"), "utf8")) : [];
    const bases = (flags.get("--base") ?? []).map((base) => resolve(cwd, base));
    const machine = currentMachine(bases, terms);
    ({ files, findings } = sanitizeTree(one("--in"), one("--out"), machine));
    log(`sanitized ${files.length} files`);
  } else if (command === "scan" && targets.length) {
    known();
    files = expand(targets, cwd);
    findings = scan(files, cwd, patterns);
  } else if (command === "words" && targets.length) {
    known("--list");
    files = expand(targets, cwd);
    findings = scan(files, cwd, sensitiveWords(readFileSync(one("--list"), "utf8")));
  } else {
    throw new UsageError(command ? `bad command line for \`${command}\`` : "no command");
  }
  report(findings).forEach((line) => log(line));
  log(
    findings.length
      ? `${findings.length} findings in ${new Set(findings.map((f) => f.file)).size} of ${files.length} files`
      : `clean: ${files.length} files`,
  );
  return findings.length ? 1 : 0;
}

if (import.meta.main) {
  try {
    process.exitCode = main(process.argv.slice(2), process.cwd());
  } catch (err) {
    console.error(err instanceof UsageError ? `${err.message}\n\n${USAGE}` : String(err));
    process.exitCode = 2;
  }
}
