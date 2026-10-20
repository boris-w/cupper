import type { EventBody } from "@cupper/protocol";
import { expect, test, type Page } from "@playwright/test";

import { open } from "./serve";

/** 12 s before the page's fixed clock (`open`). */
const STARTED = Date.parse("2026-01-01T09:00:00+08:00") - 12_000;
const turn = { turn_id: "t_1", started_at: STARTED };
const started = (item_id: string, kind: string, more: object = {}) => ({
  type: "item.started",
  data: { item_id, kind, ...turn, ...more },
});
const delta = (item_id: string, text: string, field = "text") => ({
  type: "item.delta",
  data: { item_id, field, text },
});
const done = (item_id: string, more: object = {}) => ({
  type: "item.completed",
  data: { item_id, status: "completed", ended_at: 2, duration_ms: 1200, ...more },
});
const tool = (item_id: string, tool_kind: string, more: object = {}) =>
  started(item_id, "tool_call", {
    ...{ tool_kind, title: tool_kind, paths: [], extraction: "full", output_state: "complete" },
    ...more,
  });
const header = [
  { type: "turn.started", data: { ...turn, turn_no: 1, origin: "user" } },
  started("i_0", "user_message", {
    content: [{ type: "text", text: "整理一下 README，再跑一遍测试" }],
    steered: false,
  }),
];

const ANSWER = [
  "## 改动概要",
  "README 的 **安装** 一节重写了，命令改成 `pnpm i`；详见 [贡献指南](https://example.com/contributing)。",
  "1. 安装依赖\n2. 运行 `pnpm test`\n   - 单元测试\n   - 截图测试",
  "```sh\npnpm i\npnpm test\n```",
  "| 检查 | 结果 |\n|---|---|\n| lint | 通过 |\n| test | 通过 |",
  "> 截图的基线没有变。",
  "![架构图](https://example.com/arch.png) <b>原样显示的 HTML</b>",
].join("\n\n");

const MARKDOWN = [
  ...header,
  started("i_1", "thought"),
  delta("i_1", "先看 README 的结构。"),
  done("i_1", { duration_ms: 4000 }),
  started("i_2", "agent_message"),
  delta("i_2", ANSWER),
  done("i_2"),
  {
    type: "turn.completed",
    data: {
      turn_id: "t_1",
      status: "completed",
      ended_at: 2,
      duration_ms: 8000,
      final_item_ids: [],
    },
  },
] as EventBody[];

const ESC = String.fromCharCode(27);
const TOOLS = [
  ...header,
  tool("i_1", "read", { paths: ["README.md"], group_id: "g_1" }),
  done("i_1"),
  tool("i_2", "search", { query: "pnpm install", group_id: "g_1" }),
  done("i_2"),
  tool("i_3", "read", { paths: ["package.json"], group_id: "g_1" }),
  done("i_3"),
  tool("i_4", "edit", { paths: ["README.md"] }),
  done("i_4"),
  tool("i_5", "execute", { command: "pnpm test" }),
  delta("i_5", `${ESC}[32m√${ESC}[0m 68 passed\n${ESC}[31m×${ESC}[0m 1 failed\n`, "output"),
  done("i_5", { status: "failed", exit_code: 1 }),
  started("i_6", "notice", { code: "upstream_retry", params: { attempt: 2, max: 5 } }),
  started("i_7", "error", { code: "quota_exhausted", guidance: "none", retryable: false }),
  tool("i_8", "execute", { command: "pnpm test --filter ui-core" }),
] as EventBody[];

async function show(page: Page, stream: EventBody[], theme: "light" | "dark") {
  await open(page, { info: { theme }, streams: { "session/s_1": stream } });
  await page.evaluate(() => window.cupperShell.navigate({ kind: "session", session_id: "s_1" }));
}

for (const theme of ["light", "dark"] as const) {
  test(`UI-stream-markdown ${theme}`, async ({ page }) => {
    await show(page, MARKDOWN, theme);
    await expect(page.getByText("改动概要")).toBeVisible();
    await expect(page).toHaveScreenshot(`stream-markdown-${theme}.png`);
    expect(await page.evaluate(() => window.cupperViolations)).toEqual([]);
  });

  test(`UI-stream-tools ${theme}`, async ({ page }) => {
    await show(page, TOOLS, theme);
    await page.getByText("已运行 pnpm test", { exact: true }).click();
    await page.getByText("读取了 2 个文件，搜索了 1 次").click();
    await expect(page.getByText("退出码 1")).toBeVisible();
    await expect(page).toHaveScreenshot(`stream-tools-${theme}.png`);
    expect(await page.evaluate(() => window.cupperViolations)).toEqual([]);
  });
}
