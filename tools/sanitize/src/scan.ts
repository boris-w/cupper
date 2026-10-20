/**
 * Personal data scans over test data (design 5.9.3): generic patterns that
 * need no knowledge of the machine, and lists of terms such as the
 * maintainer's local sensitive word list. A finding names the file, the line
 * and the rule; never the text that matched.
 */
import { globSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

export interface Finding {
  file: string;
  line: number;
  rule: string;
}

/** The rules one line of text breaks. */
export type Matcher = (line: string) => string[];

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
export const patterns: Matcher = (line) =>
  PATTERNS.flatMap(([rule, re]) => {
    const matches = [...line.matchAll(re)];
    return matches.some((m) => !PLACEHOLDER_DOMAIN.test(m.groups?.domain ?? "")) ? [rule] : [];
  });

/** Case-insensitive substrings, each with the rule it reports. */
export function terms(list: readonly (readonly [rule: string, term: string])[]): Matcher {
  const lowered = list.map(([rule, term]) => [rule, term.toLowerCase()] as const);
  return (line) => {
    const lower = line.toLowerCase();
    return lowered.flatMap(([rule, term]) => (term && lower.includes(term) ? [rule] : []));
  };
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

/** Files, directories (recursively) and glob patterns, relative to `cwd`. */
export function expand(targets: readonly string[], cwd: string): string[] {
  const files = new Set<string>();
  const walk = (path: string) => {
    if (statSync(join(cwd, path)).isDirectory()) {
      for (const name of readdirSync(join(cwd, path)).sort()) walk(join(path, name));
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
  return files.flatMap((file) =>
    readFileSync(join(cwd, file), "utf8")
      .split("\n")
      .flatMap((text, i) =>
        [...new Set(matcher(text))].map((rule) => ({ file, line: i + 1, rule })),
      ),
  );
}

export function report(findings: readonly Finding[]): string[] {
  return findings.map(({ file, line, rule }) => `${file}:${line}: ${rule}`);
}
