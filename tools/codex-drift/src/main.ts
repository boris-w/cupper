/**
 * `codex-drift` (design 5.1.6, ADR-08), a nightly step of `xtask ci`: whether
 * the newest codex on npm changed the app-server methods that the pinned
 * codex-acp uses, against testdata/codex-drift/baseline.json. A drift fails
 * once per codex version: reviewed, it goes into testdata/codex-drift/reviewed.json,
 * and passes while it stays the same.
 *
 *     node tools/codex-drift/src/main.ts fetch <dir>  # npm: the newest codex, and what it takes to compare
 *     node tools/codex-drift/src/main.ts check <dir>  # offline: compare it with the baseline
 *     node tools/codex-drift/src/main.ts write <dir>  # the baseline anew: the manifest's verified max codex
 *     node tools/codex-drift/src/main.ts review <dir> <task> <verdict>  # record the drift `check` found
 *
 * Only `codex app-server generate-json-schema` runs, with HOME and CODEX_HOME
 * in <dir>: no login, no session, nothing of ~/.codex. Exits 0 when nothing
 * changed or the drift was reviewed, 1 on another drift or a failure, 2 on a
 * usage error.
 */
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { compare, FILES, render, reviewed, summarize, type Review, type Summary } from "./drift.ts";

interface Pin {
  url: string;
  integrity: string;
}
/** What `fetch` found: the newest codex, and the tarballs when it is not the baseline's. */
interface Fetched {
  version: string;
  platform?: string;
  codex?: Pin;
  acp?: Pin;
}

const root = resolve(import.meta.dirname, "../../..");
const BASELINE = join(root, "testdata/codex-drift/baseline.json");
const REVIEWED = join(root, "testdata/codex-drift/reviewed.json");
const REGISTRY = "https://registry.npmjs.org/";
const CURL = `-q --fail --silent --show-error --location --proto =https --proto-redir =https
  --retry 3 --retry-all-errors --connect-timeout 30 --user-agent cupper-research`.split(/\s+/);
/** Where each platform package keeps the binary: what its `bin/codex.js` picks. */
const TRIPLES: Record<string, string> = {
  "darwin-arm64": "aarch64-apple-darwin",
  "linux-x64": "x86_64-unknown-linux-musl",
};
const manifest = JSON.parse(readFileSync(join(root, "runtime/manifest.json"), "utf8")) as {
  codex_acp: Pin & { version: string };
  cli: { codex: { verified: { max: string } } };
};
const acp = manifest.codex_acp;
const verified = manifest.cli.codex.verified.max;

/** curl as `xtask runtime-pack` runs it: no user configuration, https only, the project's User-Agent. */
function curl(url: string, ...args: string[]): string {
  if (!url.startsWith(REGISTRY)) throw new Error(`${url} is not on the npm registry`);
  return execFileSync("curl", [...CURL, ...args, url], { encoding: "utf8" });
}

const sri = (file: string) =>
  existsSync(file)
    ? `sha512-${createHash("sha512").update(readFileSync(file)).digest("base64")}`
    : "";

/** Downloads the pinned codex-acp and codex `version` for this platform into `dir`, unless there already. */
function download(dir: string, version: string): Fetched {
  const platform = `${process.platform}-${process.arch}`;
  const { dist } = JSON.parse(curl(`${REGISTRY}@openai/codex/${version}-${platform}`)) as {
    dist: { tarball: string; integrity: string };
  };
  const fetched = {
    version,
    platform,
    codex: { url: dist.tarball, integrity: dist.integrity },
    acp,
  };
  for (const name of ["codex", "acp"] as const) {
    const file = join(dir, `${name}.tgz`);
    if (sri(file) !== fetched[name].integrity) curl(fetched[name].url, "--output", file);
  }
  return fetched;
}

/** The tarballs checked against their integrity, unpacked, and codex's schema summed up. */
function summary(dir: string, fetched: Fetched): Summary {
  const work = join(dir, "work");
  rmSync(work, { recursive: true, force: true });
  mkdirSync(join(work, "home"), { recursive: true });
  const bin = `package/vendor/${TRIPLES[fetched.platform ?? ""]}/bin/codex`;
  const tarballs = [
    ["codex", fetched.codex, bin],
    ["acp", fetched.acp, "package/dist/index.js"],
  ] as const;
  for (const [name, pin, member] of tarballs) {
    const file = join(dir, `${name}.tgz`);
    if (sri(file) !== pin?.integrity)
      throw new Error(`${name}.tgz does not match ${pin?.integrity}`);
    execFileSync("tar", ["-xzf", file, "-C", work, member]);
  }
  const env = { HOME: join(work, "home"), CODEX_HOME: join(work, "home") };
  const schemas = (flavor: string, ...flags: string[]) => {
    const out = join(work, flavor);
    execFileSync(join(work, bin), ["app-server", "generate-json-schema", "--out", out, ...flags], {
      env,
    });
    return Object.fromEntries(
      Object.values(FILES).map((file) => [file, JSON.parse(readFileSync(join(out, file), "utf8"))]),
    );
  };
  const adapter = readFileSync(join(work, "package/dist/index.js"), "utf8");
  return summarize(schemas("exp", "--experimental"), schemas("stable"), adapter);
}

const [command, dir, task, verdict] = process.argv.slice(2);
const usage =
  ["fetch", "check", "write"].includes(command ?? "") || (command === "review" && verdict);
if (!dir || !usage) {
  console.error("usage: codex-drift fetch|check|write <dir> | review <dir> <task> <verdict>");
  process.exit(2);
}
mkdirSync(dir, { recursive: true });
const fetchedFile = join(dir, "fetched.json");

if (command === "write") {
  const found = summary(dir, download(dir, verified));
  const baseline = { codexAcp: acp.version, codex: verified, ...found };
  writeFileSync(BASELINE, `${JSON.stringify(baseline, null, 2)}\n`);
  console.log(
    `baseline written: codex-acp ${acp.version} uses ${Object.keys(found.methods).length} methods`,
  );
  process.exit(0);
}
const base = JSON.parse(readFileSync(BASELINE, "utf8")) as Summary & {
  codexAcp: string;
  codex: string;
};
if (command === "fetch") {
  const latest = (JSON.parse(curl(`${REGISTRY}@openai/codex/latest`)) as { version: string })
    .version;
  const fetched = latest === base.codex ? { version: latest } : download(dir, latest);
  writeFileSync(fetchedFile, JSON.stringify(fetched));
  console.log(`the newest codex on npm: ${latest}; the baseline's: ${base.codex}`);
  process.exit(0);
}
if (base.codexAcp !== acp.version || base.codex !== verified) {
  console.error(
    `the baseline is for codex-acp ${base.codexAcp} and codex ${base.codex}, the manifest pins`,
  );
  console.error(
    `codex-acp ${acp.version} and verifies codex up to ${verified}: rewrite it with \`write\``,
  );
  process.exit(1);
}
const fetched = JSON.parse(readFileSync(fetchedFile, "utf8")) as Fetched;
if (fetched.version === base.codex) {
  console.log(`no codex newer than the baseline's ${base.codex}`);
  process.exit(0);
}
const drift = render(compare(base, summary(dir, fetched)));
type Reviews = Record<string, Review>;
const reviews = existsSync(REVIEWED) ? (JSON.parse(readFileSync(REVIEWED, "utf8")) as Reviews) : {};
if (command === "review" && task && verdict) {
  const reviewedOn = new Date().toISOString().slice(0, 10);
  reviews[fetched.version] = { baseline: base.codex, reviewed: reviewedOn, task, verdict, drift };
  writeFileSync(REVIEWED, `${JSON.stringify(reviews, null, 2)}\n`);
  console.log(`the drift of codex ${fetched.version} recorded as reviewed`);
  process.exit(0);
}
const used = Object.keys(base.methods).length;
console.log(
  `codex ${fetched.version} against ${base.codex}, the ${used} methods codex-acp ${acp.version} uses:`,
);
for (const line of drift.length ? drift : ["no change"]) console.log(`  ${line}`);
const review = reviews[fetched.version];
if (drift.length && reviewed(drift, base.codex, review)) {
  console.log(`reviewed on ${review.reviewed} (${review.task}): ${review.verdict}`);
} else if (drift.length) {
  const again = `not the drift reviewed on ${review?.reviewed} (${review?.task}): review it again`;
  console.log(review ? again : "not reviewed yet: once it is, record it with `review`");
  process.exit(1);
}
