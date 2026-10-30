/**
 * Personal data scans over test data (design 5.9.3): generic patterns that
 * need no knowledge of the machine, and lists of terms such as the
 * maintainer's local sensitive word list. Each line is scanned, and so is
 * each text a recording streams over several frames, folded. A finding names
 * the file, the line or the range of lines and the rule; never the text that
 * matched.
 */
import { globSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, resolve } from "node:path";

import { fold, streams } from "./streams.ts";

export interface Finding {
  file: string;
  line: number;
  /** The last line, when the match runs over several frames. */
  to?: number;
  rule: string;
}

/** A rule a text breaks and where: from `start` to before `end`. */
export type Hit = readonly [rule: string, start: number, end: number];

export type Matcher = (text: string) => Hit[];

export const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** Every match of `re` in `text` as a hit of `rule`. */
export const hits = (rule: string, re: RegExp, text: string): Hit[] =>
  [...text.matchAll(re)].map((m) => [rule, m.index, m.index + m[0].length]);

/** Placeholder domains (RFC 2606, RFC 6761) are not personal. */
const PLACEHOLDER_DOMAIN = /(^|\.)(example|example\.(com|net|org)|invalid|test|localhost)$/i;

const PATTERNS: readonly (readonly [string, RegExp])[] = [
  // `/Users/<user>`, `/Users/$USER` and the system's `/Users/Shared` are fine.
  ["home directory path", /\/Users\/(?![<$]|Shared\b)/g],
  ["per-user temporary path", /\/var\/folders\//gi],
  ["email address", /[A-Za-z0-9._%+-]+@(?<domain>[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,})/g],
  ["API key", /\bsk-[A-Za-z0-9_-]{16,}/g],
  ["JWT", /\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+/g],
  ["bearer token", /\bbearer\s+[A-Za-z0-9._~+/=-]{8,}/gi],
  ["GitHub token", /\b(gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{20,})/g],
  ["AWS access key", /\b(AKIA|ASIA)[0-9A-Z]{16}\b/g],
  ["Slack token", /\bxox[abposr]-[A-Za-z0-9-]{10,}/g],
];

/** Generic patterns: home and temporary paths, email addresses, token shapes. */
export const patterns: Matcher = (text) =>
  PATTERNS.flatMap(([rule, re]) =>
    [...text.matchAll(re)]
      .filter((m) => !PLACEHOLDER_DOMAIN.test(m.groups?.domain ?? ""))
      .map((m): Hit => [rule, m.index, m.index + m[0].length]),
  );

/** Case-insensitive substrings, each with the rule it reports. */
export function terms(list: readonly (readonly [rule: string, term: string])[]): Matcher {
  const res = list.flatMap(([rule, t]) =>
    t ? [[rule, new RegExp(escapeRe(t), "gi")] as const] : [],
  );
  return (text) => res.flatMap(([rule, re]) => hits(rule, re, text));
}

/** Lines of a word list: one entry per line, `#` starts a comment line. */
export function wordList(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith("#"));
}

/** The sensitive word list as terms reported by their entry number, from 1. */
export function sensitiveWords(text: string): Matcher {
  return terms(
    wordList(text).map((word, i) => [`entry ${i + 1} of the sensitive word list`, word]),
  );
}

export function either(...matchers: Matcher[]): Matcher {
  return (line) => matchers.flatMap((matcher) => matcher(line));
}

/** Files, directories (recursively) and glob patterns, absolute or relative to `cwd`. */
export function expand(targets: readonly string[], cwd: string): string[] {
  const files = new Set<string>();
  const walk = (path: string) => {
    if (statSync(resolve(cwd, path)).isDirectory()) {
      for (const name of readdirSync(resolve(cwd, path)).sort()) walk(join(path, name));
    } else {
      files.add(path);
    }
  };
  for (const target of targets) {
    const paths = /[*?[]/.test(target) ? globSync(target, { cwd }).sort() : [target];
    paths.forEach(walk);
  }
  return [...files];
}

export function scan(files: readonly string[], cwd: string, matcher: Matcher): Finding[] {
  return files.flatMap((file) => {
    const lines = readFileSync(resolve(cwd, file), "utf8").split("\n");
    const found = new Map<string, Finding>();
    const add = (f: Finding) => found.set(`${f.line}-${f.to ?? f.line}:${f.rule}`, f);
    lines.forEach((text, i) => matcher(text).forEach(([rule]) => add({ file, line: i + 1, rule })));
    for (const pieces of streams(lines.map(json))) {
      const { text, owner } = fold(pieces.map(({ at, key }) => at[key]));
      for (const [rule, start, end] of matcher(text)) {
        const line = (at: number) => (pieces[owner[at] ?? 0]?.frame ?? 0) + 1;
        const [first, last] = [line(start), line(end - 1)];
        add(last > first ? { file, line: first, to: last, rule } : { file, line: first, rule });
      }
    }
    return [...found.values()].sort((a, b) => a.line - b.line);
  });
}

function json(line: string): unknown {
  try {
    return JSON.parse(line);
  } catch {
    return undefined;
  }
}

export function report(findings: readonly Finding[]): string[] {
  return findings.map(
    ({ file, line, to, rule }) => `${file}:${line}${to ? `-${to}` : ""}: ${rule}`,
  );
}
