// A one-off Vitest configuration for the pre-measurement (task material, not
// in CI). Run from the repository root:
//   pnpm --dir apps/desktop exec vitest run --config ../../docs/tasks/T076-desktop-window-bridge/premeasure.config.ts
// Nothing under docs/ has node_modules, so the packages are aliased to their
// sources and the test uses Vitest's globals.
const at = (path: string) => new URL(path, import.meta.url).pathname;
const packages = "../../../packages/host-client/src/";

export default {
  resolve: {
    alias: [
      { find: /^@cupper\/host-client\/mock$/, replacement: at(`${packages}mock/index.ts`) },
      { find: /^@cupper\/host-client$/, replacement: at(`${packages}index.ts`) },
    ],
  },
  test: {
    root: import.meta.dirname,
    include: ["premeasure.test.ts"],
    globals: true,
    testTimeout: 60_000,
  },
};
