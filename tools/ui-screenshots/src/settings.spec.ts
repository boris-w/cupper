import type { EventBody } from "@cupper/protocol";
import { expect, test } from "@playwright/test";

import { open, violations } from "./serve";

/**
 * The index the pages show: Claude running; Codex without its CLI and logged
 * out; git without git-lfs; the desktop window and the CLI, the CLI on weak
 * authentication; the project of the archived session `methods.json` lists.
 */
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
  {
    type: "clients.updated",
    data: {
      clients: [
        {
          conn_id: "c_1",
          principal_kind: "desktop_ui",
          role: "window",
          client_name: "Cupper",
          client_version: "0.1.0",
          since: 1,
          auth: "verified",
        },
        {
          conn_id: "c_2",
          principal_kind: "cli",
          client_name: "cupper-cli",
          client_version: "0.1.0",
          since: 1,
          auth: "weak",
        },
      ],
    },
  },
  {
    type: "project.upserted",
    data: {
      project_id: "p_1",
      name: "cupper",
      path: "/work/cupper",
      is_git: true,
      added_at: 1,
      position: 0,
      trust_state: "trusted",
    },
  },
];

/** Each page: its test, a section on it and what that section shows once loaded. */
const PAGES = [
  ["agents", "UI-settings-agents", "Codex", "没有找到 CLI"],
  ["archived", "UI-settings-archived", "会话", "Fix the build"],
  ["connections", "UI-settings-archived", "本机客户端", "弱认证"],
] as const;

for (const theme of ["light", "dark"] as const) {
  for (const [section, id, region, shows] of PAGES) {
    test(`${id} ${section} ${theme}`, async ({ page }) => {
      await open(page, { info: { theme }, streams: { index: INDEX as EventBody[] } });
      const target = { kind: "settings", section };
      await page.evaluate((target) => window.cupperShell.navigate(target), target);
      await expect(page.getByRole("region", { name: region })).toContainText(shows);
      await expect(page).toHaveScreenshot(`settings-${section}-${theme}.png`);
      expect(await violations(page)).toEqual([]);
    });
  }
}
