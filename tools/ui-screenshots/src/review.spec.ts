import { expect, test, type Page } from "@playwright/test";

import { open, violations } from "./serve";

const file = (path: string, additions: number, deletions: number) => ({
  ...{ path, additions, deletions },
  ...{ binary: false, change: "update" },
});
const diff = (diff_token: string, files: ReturnType<typeof file>[], caveats: string[] = []) => ({
  ...{ caveats, diff_token, files },
  stat: {
    added: files.reduce((sum, f) => sum + f.additions, 0),
    removed: files.reduce((sum, f) => sum + f.deletions, 0),
    files: files.length,
  },
});
const FILES = [
  file("crates/host/src/git/apply.rs", 42, 7),
  file("packages/ui-core/src/review/ReviewPanel.tsx", 120, 0),
  file("README.md", 2, 2),
];
type Diffs = Record<string, ReturnType<typeof diff>>;
const DIFFS: Diffs = {
  last_turn: diff("dt_t", FILES.slice(0, 2), ["mixed_changes"]),
  unstaged: diff("dt_u", FILES),
  staged: diff("dt_s", FILES.slice(2)),
};
const EMPTY: Diffs = { ...DIFFS, unstaged: diff("dt_u", []) };

/** Session s_1 with the side panel open on `diffs`; `changed` paths make the configuration notice. */
async function show(
  page: Page,
  opts: { theme?: "light" | "dark"; diffs?: Diffs; changed?: string[] } = {},
) {
  const { theme = "light", diffs = DIFFS, changed } = opts;
  await open(page, { info: { theme, prefs: { "layout.sidePanelOpen": true } } });
  await page.evaluate(
    ([diffs, changed]) => {
      window.cupperHost.handle("git/diff", ({ params }) => ({
        result: diffs[(params as { scope: string }).scope],
      }));
      const status = { ahead: 0, behind: 0, files: [], repo_config_changed: changed };
      window.cupperHost.handle("git/status", () => ({ result: status }));
      window.cupperShell.navigate({ kind: "session", session_id: "s_1" });
    },
    [diffs, changed] as const,
  );
  // By its label, as an open menu hides the rest of the page from the accessibility tree.
  const panel = page.locator('aside[aria-label="侧边面板"]');
  await expect(panel.getByRole("button", { name: "查阅来源" })).toBeVisible();
  await page.mouse.move(400, 780);
  return panel;
}

async function scope(page: Page, name: string) {
  await page.getByRole("button", { name: "查阅来源" }).click();
  await page.getByRole("menuitemradio", { name }).click();
  await expect(page.getByRole("menu")).toBeHidden();
}

for (const theme of ["light", "dark"] as const) {
  test(`UI-review-panel unstaged ${theme}: the scope, the counts and the bulk actions`, async ({
    page,
  }) => {
    const panel = await show(page, { theme });
    await expect(panel.getByRole("button", { name: "暂存全部" })).toBeVisible();
    await expect(panel).toHaveScreenshot(`review-unstaged-${theme}.png`);
    expect(await violations(page)).toEqual([]);
  });

  test(`UI-review-panel revert ${theme}: the confirmation over the pill`, async ({ page }) => {
    const panel = await show(page, { theme });
    await panel.getByRole("button", { name: "还原全部" }).click();
    await expect(panel.getByRole("alertdialog")).toHaveAttribute("data-ready", "true");
    await page.mouse.move(400, 780);
    await expect(panel).toHaveScreenshot(`review-revert-${theme}.png`);
  });
}

test("UI-review-panel last turn: caveats and the configuration notice", async ({ page }) => {
  const panel = await show(page, { changed: [".git/hooks/pre-commit", ".gitattributes"] });
  await scope(page, "最近一轮");
  await expect(panel.getByText(/混有你自己或其他会话的改动/)).toBeVisible();
  await page.mouse.move(400, 780);
  await expect(panel).toHaveScreenshot("review-last-turn.png");
});

test("UI-review-panel staged: only 取消暂存全部", async ({ page }) => {
  const panel = await show(page);
  await scope(page, "已暂存");
  await expect(panel.getByRole("button", { name: "取消暂存全部" })).toBeVisible();
  await expect(panel.getByRole("button", { name: "还原全部" })).toBeHidden();
  await page.mouse.move(400, 780);
  await expect(panel).toHaveScreenshot("review-staged.png");
});

test("UI-review-panel scope menu", async ({ page }) => {
  const panel = await show(page);
  await page.getByRole("button", { name: "查阅来源" }).click();
  await expect(page.getByRole("menuitemradio", { name: "未暂存" })).toBeVisible();
  await expect(panel).toHaveScreenshot("review-scope-menu.png");
  expect(await violations(page)).toEqual([]);
});

test("UI-review-panel empty: nothing unstaged, no pill", async ({ page }) => {
  const panel = await show(page, { diffs: EMPTY });
  await expect(panel.getByText("没有未暂存的更改")).toBeVisible();
  await expect(panel.getByRole("button", { name: "暂存全部" })).toBeHidden();
  await expect(panel).toHaveScreenshot("review-empty.png");
});
