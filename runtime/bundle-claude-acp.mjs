// Bundles claude-agent-acp into one ESM file with esbuild-wasm (ADR-08). The host runs it on the
// user's machine with the bundled Node, after installing the npm tree from the pinned lockfile;
// `cargo xtask runtime-pack` runs it the same way to pin the bundle's sha-256 in the manifest.
// Output paths are relative to <root>, so the bundle is the same bytes wherever <root> is.
//
// Usage: node bundle-claude-acp.mjs <esbuild-wasm/lib/main.js> <root> <entry> <out>
//        <entry> and <out> are relative to <root>.
import { createRequire } from "node:module";
import { join } from "node:path";
import process from "node:process";

const [esbuildMain, root, entry, out] = process.argv.slice(2);
if (out === undefined) {
  process.stderr.write(
    "usage: bundle-claude-acp.mjs <esbuild-wasm main.js> <root> <entry> <out>\n",
  );
  process.exit(2);
}
const esbuild = createRequire(esbuildMain)(esbuildMain);
await esbuild.build({
  absWorkingDir: root,
  entryPoints: [join(root, entry)],
  outfile: join(root, out),
  bundle: true,
  platform: "node",
  format: "esm",
  target: "node22",
  legalComments: "eof",
  logLevel: "warning",
  // CommonJS dependencies in an ESM bundle still call require() for Node builtins.
  banner: {
    js: "import { createRequire as __cr } from 'node:module'; const require = __cr(import.meta.url);",
  },
});
