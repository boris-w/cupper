import type { EventBody } from "@cupper/protocol";
import { expect, test, type Page } from "@playwright/test";

import { open, violations } from "./serve";

const project = (project_id: string, name: string, position: number, trust_state: string) => ({
  type: "project.upserted",
  data: {
    project_id,
    name,
    position,
    trust_state,
    path: `/work/${name}`,
    is_git: true,
    added_at: 1,
  },
});

/** Claude whose CLI is not found. */
const AGENTS = {
  type: "agents.updated",
  data: {
    agents: [
      {
        agent: "claude",
        state: "unavailable",
        adapter: { runtime: "ready", version: "0.91.0" },
        auth: { state: "unchecked" },
        config_dir: "default",
      },
    ],
    git: {},
  },
};
const PROJECTS = [
  project("p_1", "cupper", 0, "trusted"),
  project("p_2", "notes", 1, "untrusted"),
  AGENTS,
];

/** Each state: the index, and what to open before the shot. */
const STATES: [string, object[], (page: Page) => Promise<void>][] = [
  ["projects", PROJECTS, async () => undefined],
  ["no-project", [AGENTS], async () => undefined],
  ["project-picker", PROJECTS, (page) => page.getByRole("button", { name: "选择项目" }).click()],
  ["agent-picker", PROJECTS, (page) => page.getByRole("button", { name: "选择 agent" }).click()],
];

for (const theme of ["light", "dark"] as const) {
  for (const [state, index, act] of STATES) {
    test(`UI-new-session ${state} ${theme}`, async ({ page }) => {
      await open(page, { info: { theme }, streams: { index: index as EventBody[] } });
      await act(page);
      await expect(page).toHaveScreenshot(`new-session-${state}-${theme}.png`);
      expect(await violations(page)).toEqual([]);
    });
  }
}
