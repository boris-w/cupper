/**
 * `rec-shape-diff` (design 5.9.5): the shape of an adapter's new recording
 * against the old one, for the review of an upgrade.
 *
 *     node tools/rec-shape-diff/src/main.ts [--json] <old.jsonl> <new.jsonl>
 *
 * Prints what each kind gained (`+`) and lost (`-`), or with `--json` the
 * same as one JSON object. Exits 0 when the shapes are the same, 1 when
 * they differ, 2 on a usage error.
 */
import { readFileSync } from "node:fs";

import { diff, empty, KINDS, shape } from "./shape.ts";

const args = process.argv.slice(2);
const json = args[0] === "--json";
const [was, now] = json ? args.slice(1) : args;
if (was === undefined || now === undefined || args.length !== (json ? 3 : 2)) {
  console.error("usage: rec-shape-diff [--json] <old.jsonl> <new.jsonl>");
  process.exit(2);
}
const found = diff(shape(readFileSync(was, "utf8")), shape(readFileSync(now, "utf8")));
if (json) {
  console.log(JSON.stringify(found, null, 2));
} else if (empty(found)) {
  console.log("the same shape");
} else {
  for (const kind of KINDS) {
    const { added, removed } = found[kind];
    if (!added.length && !removed.length) continue;
    console.log(`${kind}:`);
    added.forEach((item) => console.log(`  + ${item}`));
    removed.forEach((item) => console.log(`  - ${item}`));
  }
}
process.exit(empty(found) ? 0 : 1);
