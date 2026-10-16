#!/usr/bin/env node
// Print size, sha256 (hex) and sha512 (SRI, as used by npm "integrity") of files.
// Usage: node hashfile.mjs <file>...
import { createHash } from "node:crypto";
import { createReadStream, statSync } from "node:fs";
import { basename } from "node:path";

async function hash(file) {
  const h256 = createHash("sha256");
  const h512 = createHash("sha512");
  for await (const chunk of createReadStream(file)) {
    h256.update(chunk);
    h512.update(chunk);
  }
  return { file: basename(file), size: statSync(file).size, sha256: h256.digest("hex"), sri: `sha512-${h512.digest("base64")}` };
}

for (const f of process.argv.slice(2)) console.log(JSON.stringify(await hash(f)));
