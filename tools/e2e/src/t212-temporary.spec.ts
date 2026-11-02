// T212, temporary: one failing test in each of the two runs, to see both
// runs' attachments in the failed job's artifact. Reverted after the run.

import { expect, test } from "./fixture";

test(
  "T212 temporary: fails on purpose in the rest's run",
  { tag: "@smoke" },
  async ({ cupper }) => {
    await cupper.launch();
    await cupper.connected();
    expect("the rest's run").toBe("failed on purpose");
  },
);

test(
  "T212 temporary: fails on purpose in the focus run",
  { tag: ["@focus", "@smoke"] },
  async ({ cupper }) => {
    await cupper.launch();
    await cupper.connected();
    expect("the focus run").toBe("failed on purpose");
  },
);
