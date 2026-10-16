// SEA entry for claude-agent-acp 0.91.0.
// Node 24's SEA only runs a CommonJS main, and the adapter's own dist/index.js
// uses top-level await, which esbuild cannot emit as CommonJS. This file mirrors
// the server branch of dist/index.js with the await moved into an async
// function. ADAPTER_DIST is mapped to the installed package's dist directory
// with `bundle.mjs --alias ADAPTER_DIST=<.../claude-agent-acp/dist>`.
import { applyManagedPolicyEnv } from "ADAPTER_DIST/managed-policy.js";
import { serveAcp } from "ADAPTER_DIST/serve.js";
import packageJson from "ADAPTER_DIST/../package.json";

async function main() {
  if (process.argv.includes("--version") || process.argv.includes("-v")) {
    console.log(packageJson.version);
    process.exit(0);
  }
  await applyManagedPolicyEnv();
  console.log = console.error;
  console.info = console.error;
  console.warn = console.error;
  console.debug = console.error;
  process.on("unhandledRejection", (reason, promise) => {
    console.error("Unhandled Rejection at:", promise, "reason:", reason);
  });
  const acp = serveAcp(undefined, { experimentalV2: process.env.CLAUDE_AGENT_ACP_EXPERIMENTAL_V2 === "1" });
  const shutdown = async () => {
    await acp.dispose().catch((err) => console.error("Error during cleanup:", err));
    process.exit(0);
  };
  acp.closed.then(shutdown);
  process.on("SIGTERM", shutdown);
  process.on("SIGINT", shutdown);
  process.stdin.resume();
}

main();
