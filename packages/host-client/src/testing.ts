// Shared by the tests; Node only, and not exported by the package.
import { readFileSync } from "node:fs";

import { zEventBody } from "@cupper/protocol";

import type { MethodFixture } from "./mock/host";
import { IndexView } from "./views/index-view";
import { SessionView } from "./views/session-view";

const read = (path: string | URL): unknown => JSON.parse(readFileSync(path, "utf8"));

/** `protocol/fixtures/methods.json`, the samples Rust checks too. */
export const fixtures = read(
  new URL("../../../protocol/fixtures/methods.json", import.meta.url),
) as MethodFixture[];

/** Event bodies and the view JSON Rust folds them into (`cupper-protocol` `view::parity`). */
export interface ViewCase {
  name: string;
  view: "index" | "session";
  ops: unknown[];
  expected: unknown;
}

export const viewCases = (path: string | URL) => read(path) as ViewCase[];

/** The JSON of the view `ops` fold into, each op parsed as T087 hands it over. */
export function fold({ view, ops }: ViewCase): unknown {
  const folded = view === "index" ? new IndexView() : new SessionView();
  ops.forEach((op) => folded.apply(zEventBody.parse(op)));
  return JSON.parse(JSON.stringify(folded));
}
