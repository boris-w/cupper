/**
 * The second step of `pnpm gen` (design 5.1.5, 5.3.10): generates
 * `packages/protocol/src/gen/` from `protocol/schema/v1.json`. hey-api writes
 * the TS types and the zod schemas, which strip unknown fields rather than
 * reject them (not strict); {@link postprocess} adapts them to the open types;
 * Prettier formats the files with the repository's settings.
 */
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { createClient } from "@hey-api/openapi-ts";
import * as prettier from "prettier";

import { HEADER, postprocess, toOpenApi, type Schema } from "./postprocess.ts";

const root = resolve(import.meta.dirname, "../../..");
const out = join(root, "packages/protocol/src/gen");
const schema = JSON.parse(await readFile(join(root, "protocol/schema/v1.json"), "utf8")) as Schema;

const scratch = await mkdtemp(join(tmpdir(), "gen-protocol-ts-"));
try {
  await createClient({
    input: { path: toOpenApi(schema) },
    output: { path: scratch, entryFile: false, header: HEADER },
    logs: { level: "warn", file: false },
    plugins: ["@hey-api/typescript", "zod"],
  });
  const generated = {
    types: await readFile(join(scratch, "types.gen.ts"), "utf8"),
    zod: await readFile(join(scratch, "zod.gen.ts"), "utf8"),
  };
  const files = postprocess(schema, generated);
  await rm(out, { recursive: true, force: true });
  await mkdir(out, { recursive: true });
  for (const [name, text] of Object.entries(files)) {
    const path = join(out, name);
    const options = await prettier.resolveConfig(path);
    await writeFile(path, await prettier.format(text, { ...options, filepath: path }));
  }
} finally {
  await rm(scratch, { recursive: true, force: true });
}
