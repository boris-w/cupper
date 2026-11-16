// The window's layout and its two themes after T106's check (T152): sizes
// and colours by the DOM, where a screenshot's pixel threshold would miss a
// few shades or pixels, and the way back to the bottom in both themes.

import type { EventBody } from "@cupper/protocol";
import { expect, test, type Page } from "@playwright/test";

import { open } from "./serve";

/** Turn 1 runs: the user's message, then an answer long enough to scroll. */
const turn = { turn_id: "t_1", started_at: Date.parse("2026-01-01T09:00:00+08:00") - 12_000 };
const RUNNING: EventBody[] = [
  { type: "turn.started", data: { ...turn, turn_no: 1, origin: "user" } },
  {
    type: "item.started",
    data: {
      item_id: "i_0",
      kind: "user_message",
      ...turn,
      content: [{ type: "text", text: "整理一下 README" }],
      steered: false,
    },
  },
  { type: "item.started", data: { item_id: "i_1", kind: "agent_message", ...turn } },
  {
    type: "item.delta",
    data: {
      item_id: "i_1",
      field: "text",
      text: Array.from({ length: 40 }, (_, n) => `第 ${n + 1} 段：README 的安装一节重写了。`).join(
        "\n\n",
      ),
    },
  },
] as EventBody[];

async function session(page: Page, theme: "light" | "dark") {
  await open(page, { info: { theme }, streams: { "session/s_1": RUNNING } });
  await page.evaluate(() => window.cupperShell.navigate({ kind: "session", session_id: "s_1" }));
  await expect(page.getByText("第 40 段", { exact: false })).toBeAttached();
}

const box = (page: Page, selector: string) =>
  page
    .locator(selector)
    .first()
    .evaluate((el) => el.getBoundingClientRect().toJSON() as DOMRect);
const background = (page: Page, selector: string) =>
  page
    .locator(selector)
    .first()
    .evaluate((el) => getComputedStyle(el).backgroundColor);

test("UI-layout: the title bar, and the sidebar between 188 and 468", async ({ page }) => {
  await open(page);
  const sidebar = page.getByRole("complementary", { name: "侧边栏" });
  expect((await sidebar.boundingBox())?.width).toBe(288);
  expect((await box(page, "header"))?.height).toBe(44);
  const handle = page.getByRole("separator", { name: "调整侧边栏宽度" });
  await handle.focus();
  await page.keyboard.press("Home");
  expect((await sidebar.boundingBox())?.width).toBe(188);
  await page.keyboard.press("End");
  expect((await sidebar.boundingBox())?.width).toBe(468);
});

test("UI-layout: the way back to the bottom, 32 round, 24 above the composer; 40 while a turn runs", async ({
  page,
}) => {
  await session(page, "light");
  const list = page.locator("[data-stream-list]");
  await list.hover();
  await page.mouse.wheel(0, -2000);
  const down = page.getByRole("button", { name: "回到底部" });
  await expect(down).toBeVisible();
  const button = (await down.boundingBox()) ?? { x: 0, y: 0, width: 0, height: 0 };
  const composer = await box(page, "[data-composer]");
  expect([button.width, button.height]).toEqual([40, 32]);
  expect(composer.top - (button.y + button.height)).toBe(24);
  expect(button.x + button.width / 2).toBeCloseTo(composer.left + composer.width / 2, 0);
  await down.hover();
  await expect.poll(async () => (await down.boundingBox())?.width).toBe(32);
});

for (const theme of ["light", "dark"] as const) {
  test(`UI-theme ${theme}: a running turn scrolled up`, async ({ page }) => {
    await session(page, theme);
    await page.locator("[data-stream-list]").hover();
    await page.mouse.wheel(0, -2000);
    await expect(page.getByRole("button", { name: "回到底部" })).toBeVisible();
    await expect(page).toHaveScreenshot(`theme-running-${theme}.png`);
  });
}

test("UI-theme: T106's dark values, and the light ones back on a change of theme", async ({
  page,
}) => {
  await session(page, "dark");
  expect(await background(page, "body")).toBe("rgb(24, 24, 24)");
  expect(await background(page, "[data-composer]")).toBe("rgb(54, 54, 54)");
  expect(await page.locator("body").evaluate((el) => getComputedStyle(el).color)).toBe(
    "rgb(223, 223, 223)",
  );
  await page.evaluate(() => window.cupperShell.setTheme("light"));
  await expect.poll(() => background(page, "[data-composer]")).toBe("rgb(255, 255, 255)");
  expect(await background(page, "body")).toBe("rgb(255, 255, 255)");
});
