/**
 * The test entry points, which e2e builds alone contain (design 5.9.9,
 * 5.10.7): `window.__cupperTest` and `__cupperGate` (T078), the main
 * process's `__cupperE2E` (disconnecting a window) and `__e2eDialogs` (the
 * dialogs' stand-ins), `CUPPER_E2E_DIR`, and `CUPPER_E2E_BACKGROUND` (a test's
 * app out of the foreground, T195). The scans of a build look for them:
 * flavors.test.ts in each flavor's code, IT-selfuse-build in Cupper Local.
 */
export const TEST_ENTRY_POINTS = [
  "__cupperTest",
  "__cupperGate",
  "__cupperE2E",
  "__e2eDialogs",
  "CUPPER_E2E_DIR",
  "CUPPER_E2E_BACKGROUND",
];
