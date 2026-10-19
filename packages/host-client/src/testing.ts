// Shared by the tests; Node only, and not exported by the package.
import { readFileSync } from "node:fs";

import type { MethodFixture } from "./mock/host";

/** `protocol/fixtures/methods.json`, the samples Rust checks too. */
export const fixtures = JSON.parse(
  readFileSync(new URL("../../../protocol/fixtures/methods.json", import.meta.url), "utf8"),
) as MethodFixture[];
