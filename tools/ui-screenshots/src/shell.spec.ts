import { expect, test } from "@playwright/test";

import { open, violations } from "./serve";

const panelOpen = { "layout.sidePanelOpen": true };

for (const theme of ["light", "dark"] as const) {
  test(`UI-shell ${theme}`, async ({ page }) => {
    await open(page, { info: { theme, prefs: panelOpen } });
    await expect(page).toHaveScreenshot(`shell-${theme}.png`);
  });
}

test("UI-shell-narrow floats the sidebar below 768, until Esc", async ({ page }) => {
  await page.setViewportSize({ width: 700, height: 820 });
  await open(page);
  const sidebar = page.getByRole("complementary", { name: "侧边栏" });
  await expect(sidebar).toBeHidden();
  await page.getByRole("button", { name: "切换侧边栏" }).click();
  await expect(sidebar).toBeVisible();
  await expect(page).toHaveScreenshot("shell-narrow-overlay.png");
  await page.keyboard.press("Escape");
  await expect(sidebar).toBeHidden();
});

test("UI-shell-narrow collapses the side panel while it does not fit", async ({ page }) => {
  // 288 + 352 + 320 = 960.
  await page.setViewportSize({ width: 959, height: 820 });
  await open(page, { info: { prefs: panelOpen } });
  const panel = page.getByRole("complementary", { name: "侧边面板" });
  await expect(panel).toBeHidden();
  await expect(page).toHaveScreenshot("shell-collapsed.png");
  await page.setViewportSize({ width: 960, height: 820 });
  await expect(panel).toBeVisible();
});

test("UI-csp-zero: no violation under the main document's CSP", async ({ page }) => {
  await open(page, { info: { prefs: panelOpen } });
  await page.getByRole("button", { name: "切换侧边面板" }).click();
  await page.getByRole("button", { name: "切换侧边栏" }).click();
  await page.evaluate(() => window.cupperShell.setTheme("dark"));
  // Radix's menus and tooltips render into portals and insert their own styles.
  await page.getByRole("button", { name: "会话菜单" }).click();
  await expect(page.getByRole("menu")).toBeVisible();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "切换侧边面板" }).focus();
  await expect(page.getByRole("tooltip")).toBeVisible();
  expect(await violations(page)).toEqual([]);
});
