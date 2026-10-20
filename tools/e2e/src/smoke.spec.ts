// The fixture's own check, the PR's smoke subset: the app starts its host and
// the window shows the host's data, as the renderer gets it. A window that
// only connects is not enough (T167 found two such failures).

import { expect, test } from "./fixture";
import { hostClient } from "./helpers";

test(
  "fixture: the app starts the host, and the sidebar shows a project the CLI added",
  { tag: "@smoke" },
  async ({ cupper }) => {
    await cupper.launch();
    const status = await cupper.connected();
    expect(status.running_turns).toBe(0);
    await expect.poll(hostClient(cupper).state).toEqual({ status: "ready" });
    await cupper.cli.ok(["new", "--agent", "claude", "--project", cupper.repo, "say hi"]);
    const project = cupper.window.getByRole("region", { name: "repo" });
    await expect(project.getByRole("button", { name: /say hi/ })).toBeVisible();
  },
);
