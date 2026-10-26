// A one-off Vitest configuration for the cost of the bridge's inbound copy
// (task material, not in CI). Run from the repository root:
//   pnpm --dir apps/desktop exec vitest run --config ../../docs/tasks/T078-bridge-gate/copy.config.ts
const at = (path: string) => new URL(path, import.meta.url).pathname;

export default {
  resolve: {
    alias: [{ find: /^@cupper\/host-client$/, replacement: at("../../../packages/host-client/src/index.ts") }],
  },
  test: { root: import.meta.dirname, include: ["copy.test.ts"], globals: true, testTimeout: 60_000 },
};
