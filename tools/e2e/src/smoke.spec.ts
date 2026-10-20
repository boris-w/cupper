// The fixture's own check, the PR's smoke subset: the app starts its host,
// the window connects, and the CLI sees both.

import { expect, test } from "./fixture";

test(
  "fixture: the app starts the host, the window connects, the CLI sees it",
  { tag: "@smoke" },
  async ({ cupper }) => {
    await cupper.launch();
    const status = await cupper.connected();
    expect(status.running_turns).toBe(0);
    expect(await cupper.window.locator("#root").count()).toBe(1);
  },
);
