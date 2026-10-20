import type { EventBody } from "@cupper/protocol";
import { expect, test, type Page } from "@playwright/test";

import { open } from "./serve";

const option = (option_id: string, name: string, scope: string, extra: object = {}) => ({
  ...{ option_id, name, kind: "allow_once", requires: "write" },
  ...{ effect: { scope, persistent: false }, ...extra },
});
const requested = (request: object) =>
  ({
    type: "interaction.requested",
    data: { request_id: "r_1", digest: "d", created_at: 1, actionable_by: ["desktop_ui"], request },
  }) as EventBody;

const PERMISSION = requested({
  ...{ type: "permission", title: "Run the test suite", content_truncated: false },
  tool: {
    tool_kind: "execute",
    paths: [],
    command: { type: "text", text: "cargo test --workspace" },
  },
  options: [
    option("o1", "允许", "once"),
    option("o2", "本会话内都允许", "session", {
      ...{ requires: "privileged", kind: "allow_always" },
      effect: { scope: "session", persistent: true },
    }),
    option("o3", "拒绝", "reject", { kind: "reject_once" }),
  ],
});
const PLAN = requested({
  ...{ type: "plan_confirm", plan: { type: "text", text: "1. 先补测试\n2. 再改实现" } },
  options: [option("p1", "实施", "plan_accept"), option("p2", "继续修改计划", "plan_revise")],
});
const QUESTION = requested({
  ...{ type: "question", prompt: "用哪种数据库？", unsupported_schema: false },
  fields: [
    {
      ...{
        field_id: "db",
        label: "数据库",
        type: "single_choice",
        required: true,
        allow_custom: true,
      },
      choices: [
        { label: "SQLite", value: "sqlite" },
        { label: "Postgres", value: "pg" },
      ],
    },
  ],
});

/**
 * Session s_1. With `live`, the composer has the focus when the request
 * arrives as a live event (keyboard activation); otherwise the request is
 * there from the start, and the panel waits for Tab or a click.
 */
async function show(
  page: Page,
  request: EventBody,
  live: boolean,
  theme: "light" | "dark" = "light",
) {
  await open(page, { info: { theme }, streams: { "session/s_1": live ? [] : [request] } });
  await page.evaluate(() => window.cupperShell.navigate({ kind: "session", session_id: "s_1" }));
  if (live) {
    await page.getByRole("textbox").focus();
    await page.evaluate((body) => {
      const events = [{ seq: 1, v: 1, ts: 1, ...body }];
      window.cupperHost.notify("stream/events", { stream: "session/s_1", events });
    }, request);
  }
  const panel = page.locator("[data-request-panel]");
  await expect(panel).toHaveAttribute("data-ready", "true");
  await expect(panel).toHaveAttribute("data-activation", live ? "auto" : "manual");
  await page.mouse.move(650, 780);
}

for (const theme of ["light", "dark"] as const) {
  test(`UI-request-panel permission, ready, ${theme}`, async ({ page }) => {
    await show(page, PERMISSION, true, theme);
    await expect(page).toHaveScreenshot(`request-permission-${theme}.png`);
  });
}

test("UI-request-panel permission, waiting for Tab or a click", async ({ page }) => {
  await show(page, PERMISSION, false);
  await expect(page.getByText("按 Tab 或点击以处理请求")).toBeVisible();
  await expect(page).toHaveScreenshot("request-manual.png");
});

for (const [name, request] of [
  ["plan", PLAN],
  ["question", QUESTION],
] as const) {
  test(`UI-request-panel ${name}`, async ({ page }) => {
    await show(page, request, true);
    await expect(page).toHaveScreenshot(`request-${name}.png`);
  });
}

const resolve = (request_id: string, outcome?: object, status = "resolved") =>
  ({
    type: "interaction.resolved",
    data: { request_id, status, ...(outcome ? { outcome } : {}) },
  }) as EventBody;
const asked = (event: EventBody, request_id: string) =>
  ({ ...event, data: { ...(event.data as object), request_id, turn_id: "t_1" } }) as EventBody;
const REPLY = {
  type: "item.started",
  data: {
    ...{ item_id: "i_1", turn_id: "t_1", started_at: 2, kind: "user_message", steered: false },
    ...{ content: [{ type: "text", text: "先别跑测试，看一下配置" }], reply_to: "r_2" },
  },
};

/** One turn with an approval, a rejection and its follow-up, an answered question and an expired plan. */
const RECORDS = [
  // 72 s before the screenshots' fixed clock (2026-01-01 09:00 +08:00).
  {
    type: "turn.started",
    data: { turn_id: "t_1", turn_no: 1, origin: "user", started_at: 1767229128000 },
  },
  asked(PERMISSION, "r_1"),
  resolve("r_1", { type: "selected", option_id: "o1" }),
  asked(PERMISSION, "r_2"),
  resolve("r_2", { type: "selected", option_id: "o3" }),
  REPLY,
  asked(QUESTION, "r_3"),
  resolve("r_3", { type: "answer", values: { db: "sqlite" } }),
  asked(PLAN, "r_4"),
  resolve("r_4", undefined, "expired"),
] as EventBody[];

for (const theme of ["light", "dark"] as const) {
  test(`UI-request-records ${theme}: the records, and a panel with two waiting`, async ({
    page,
  }) => {
    const waiting = [asked(PERMISSION, "r_5"), asked(PLAN, "r_6")];
    await open(page, { info: { theme }, streams: { "session/s_1": [...RECORDS, ...waiting] } });
    await page.evaluate(() => window.cupperShell.navigate({ kind: "session", session_id: "s_1" }));
    await expect(page.locator("[data-request-count]")).toHaveText("1 / 2");
    await expect(page.getByText("附于对「Run the test suite」的拒绝")).toBeVisible();
    await expect(page.locator("[data-request-panel]")).toHaveAttribute("data-ready", "true");
    await page.mouse.move(650, 780);
    await expect(page).toHaveScreenshot(`request-records-${theme}.png`);
  });
}
