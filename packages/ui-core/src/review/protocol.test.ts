import { describe, expect, test } from "vitest";

import { parse, patchOf, zFromReview, zToReview } from "./protocol";

const file = { path: "src/a.ts", additions: 2, deletions: 1, binary: false, change: "update" };

describe("the review document's protocol", () => {
  test("a message that does not fit its schema is dropped", () => {
    expect(parse(zFromReview, { type: "ready", v: 1 })).toEqual({ type: "ready", v: 1 });
    expect(parse(zFromReview, { type: "ready", v: 2 })).toBeUndefined();
    expect(parse(zFromReview, { type: "git/revert" })).toBeUndefined();
    const action = { type: "hunk_action", diff_token: "dt_1", action: "revert", path: "a" };
    expect(parse(zFromReview, { ...action, hunk_ids: [] })).toBeUndefined();
    expect(parse(zFromReview, { ...action, action: "commit" })).toBeUndefined();
    expect(parse(zToReview, { type: "render", diff_token: "dt_1", path: "a" })).toBeUndefined();
  });

  test("a file's patch keeps the host's lines under their @@ lines", () => {
    const hunk = { hunk_id: "hk_1", unit: "hunk", header: "@@ -1,2 +1,2 @@ fn a()" };
    const mode = { hunk_id: "hk_0", unit: "mode", header: "old mode 100644\nnew mode 100755" };
    const lines = [" a", "-<b>", "+<i>"];
    expect(
      patchOf(file, [
        { ...mode, lines: [] },
        { ...hunk, lines },
      ]),
    ).toBe(["--- a/src/a.ts", "+++ b/src/a.ts", hunk.header, ...lines, ""].join("\n"));
    const added = { ...file, change: "add" };
    const whole = {
      hunk_id: "hk_2",
      unit: "whole_file",
      header: "new file mode 100644\n@@ -0,0 +1 @@",
    };
    expect(patchOf(added, [{ ...whole, lines: ["+x"] }]).split("\n")).toEqual([
      "--- /dev/null",
      "+++ b/src/a.ts",
      "@@ -0,0 +1 @@",
      "+x",
      "",
    ]);
  });

  test("a whole file's lines go under each of its @@ lines by their counts", () => {
    const header = "@@ -1,2 +1,2 @@\n@@ -9 +9,2 @@";
    const lines = [" a", "-b", "+c", "\\ No newline at end of file", "-y", "+z", "+w"];
    const whole = { hunk_id: "hk_3", unit: "whole_file", header, lines };
    expect(patchOf(file, [whole]).split("\n").slice(2, -1)).toEqual([
      "@@ -1,2 +1,2 @@",
      " a",
      "-b",
      "+c",
      "\\ No newline at end of file",
      "@@ -9 +9,2 @@",
      "-y",
      "+z",
      "+w",
    ]);
  });
});
