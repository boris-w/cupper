/**
 * `schema-compat` (design 5.12.5, 5.1.6), run by `xtask ci protocol`:
 *
 *     node tools/schema-compat/src/main.ts <released.json> <current.json>
 *     node tools/schema-compat/src/main.ts --integers <schema.json>...
 *
 * The first fails on a breaking change and lists the additive ones; the
 * second fails on an integer not bounded within ±(2^53 − 1).
 */
import { readFileSync } from "node:fs";

import { compare, unsafeIntegers, type Doc } from "./compat.ts";

const read = (path: string): unknown => JSON.parse(readFileSync(path, "utf8"));
const args = process.argv.slice(2);

if (args[0] === "--integers" && args.length > 1) {
  const unsafe = args
    .slice(1)
    .flatMap((path) => unsafeIntegers(read(path)).map((at) => `${path}${at}`));
  unsafe.forEach((at) => console.error(`integer beyond 2^53 - 1 or with a format: ${at}`));
  process.exit(unsafe.length > 0 ? 1 : 0);
} else if (args.length === 2) {
  const findings = compare(read(args[0] ?? "") as Doc, read(args[1] ?? "") as Doc);
  const breaking = findings.filter((f) => f.breaking);
  for (const { at, change, breaking } of findings) {
    console.log(`${breaking ? "BREAKING" : "additive"}  ${at}: ${change}`);
  }
  console.log(`${breaking.length} breaking, ${findings.length - breaking.length} additive`);
  process.exit(breaking.length > 0 ? 1 : 0);
} else {
  console.error(
    "usage: schema-compat <released.json> <current.json> | --integers <schema.json>...",
  );
  process.exit(2);
}
