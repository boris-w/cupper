import type { EventBody } from "@cupper/protocol";
import { expect, test } from "@playwright/test";

import { open, violations } from "./serve";

/** Claude running; Codex without its CLI and logged out; git without git-lfs. */
const INDEX = [
  {
    type: "agents.updated",
    data: {
      agents: [
        {
          ...{ agent: "claude", state: "running", rss_bytes: 734003200, config_dir: "default" },
          cli: {
            path: "/opt/homebrew/bin/claude",
            version: "2.1.296",
            compat: "verified",
            install: "native",
          },
          adapter: { version: "0.91.0", runtime: "ready" },
          auth: { state: "logged_in", kind: "subscription", plan: "max" },
        },
        {
          ...{ agent: "codex", state: "unavailable", config_dir: "default" },
          adapter: { version: "2.2.2", runtime: "ready" },
          auth: { state: "logged_out" },
        },
      ],
      git: { version: "2.50.1" },
    },
  },
];

for (const theme of ["light", "dark"] as const) {
  test(`UI-settings-agents ${theme}`, async ({ page }) => {
    await open(page, { info: { theme }, streams: { index: INDEX as EventBody[] } });
    await page.evaluate(() => window.cupperShell.navigate({ kind: "settings", section: "agents" }));
    await expect(page.getByRole("region", { name: "Codex" })).toContainText("没有找到 CLI");
    await expect(page).toHaveScreenshot(`settings-agents-${theme}.png`);
    expect(await violations(page)).toEqual([]);
  });
}
