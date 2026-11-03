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

const CODE = [
  "```ts",
  "// 读取配置，缺省时用默认值",
  'import { readFile } from "node:fs/promises";',
  "",
  "export async function load(path: string, retries = 3): Promise<Config> {",
  '  const text = await readFile(path, "utf8");',
  "  return { ...JSON.parse(text), retries, debug: false };",
  "}",
  "```",
  "```html",
  '<a class="link" href="https://example.com/docs">文档</a>',
  "```",
  "```python",
  "def total(items: list[int]) -> int:",
  '    """求和"""',
  "    return sum(items, start=0)",
  "```",
].join("\n");

const completed = (turn_id: string, status = "completed") => ({
  type: "turn.completed",
  data: { turn_id, status, ended_at: 2, duration_ms: 8000, final_item_ids: [] },
});

const HIGHLIGHT = [
  ...header,
  started("i_1", "agent_message"),
  delta("i_1", CODE),
  done("i_1"),
  completed("t_1"),
] as EventBody[];

const step = (content: string, status: string) => ({ content, status });
/** A running turn whose plan is at step 2 of 4, under work that pushed it out of view. */
const PLAN_STEPS = [
  ...header,
  started("i_1", "plan", {
    source: "entries",
    entries: [
      step("读一遍 README，列出过时的段落", "completed"),
      step("重写安装一节", "in_progress"),
      step("补上贡献指南的链接", "pending"),
      step("跑一遍全部测试", "pending"),
    ],
  }),
  ...Array.from({ length: 16 }, (_, n) => n + 1).flatMap((n) => [
    tool(`i_t${n}`, "read", { paths: [`docs/part-${n}.md`] }),
    done(`i_t${n}`),
  ]),
  started("i_2", "agent_message"),
  delta("i_2", "安装一节改成了 `pnpm i`，接下来补链接。"),
  done("i_2"),
  tool("i_3", "edit", { paths: ["README.md"] }),
] as EventBody[];

const PLAN_MARKDOWN = [
  ...header,
  started("i_1", "plan", {
    source: "markdown",
    markdown: {
      type: "text",
      text: "1. 先补测试\n2. 再改实现，`cargo test` 通过后提交",
    },
  }),
  done("i_1"),
  started("i_2", "agent_message"),
  delta("i_2", "计划写好了，等你确认。"),
  done("i_2"),
  completed("t_1"),
] as EventBody[];

const error = (item_id: string, code: string, guidance: string, more: object = {}) =>
  started(item_id, "error", { code, guidance, retryable: guidance === "retry", ...more });
/** Every recovery step once; the retry is in the last turn, which can send its message again. */
const ERRORS = [
  ...header,
  error("i_1", "not_logged_in", "run_login_in_terminal", { raw: "Authentication required" }),
  error("i_2", "cli_too_old", "upgrade_cli"),
  error("i_3", "cli_missing", "install_cli"),
  error("i_4", "quota_exhausted", "quota_exhausted"),
  error("i_5", "model_unsupported", "change_model"),
  error("i_6", "adapter_crashed", "restart_agent"),
  error("i_7", "other", "page_admin", { raw: "unknown guidance falls back to none" }),
  error("i_8", "restartable", "retry"),
  completed("t_1", "failed"),
] as EventBody[];

async function show(page: Page, stream: EventBody[], theme: "light" | "dark") {
  await open(page, { info: { theme }, streams: { "session/s_1": stream } });
  await page.evaluate(() => window.cupperShell.navigate({ kind: "session", session_id: "s_1" }));
}

for (const theme of ["light", "dark"] as const) {
  test(`UI-stream-markdown ${theme}`, async ({ page }) => {
    await show(page, MARKDOWN, theme);
    await expect(page.getByText("改动概要")).toBeVisible();
    await expect(page.locator("pre[data-highlighted]")).toBeVisible();
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

  test(`UI-code-highlight ${theme}`, async ({ page }) => {
    await show(page, HIGHLIGHT, theme);
    await expect(page.locator("pre[data-highlighted]")).toHaveCount(3);
    await expect(page).toHaveScreenshot(`stream-code-${theme}.png`);
    expect(await page.evaluate(() => window.cupperViolations)).toEqual([]);
  });

  test(`UI-plan steps and pill ${theme}`, async ({ page }) => {
    await show(page, PLAN_STEPS, theme);
    const first = page.getByText("读一遍 README，列出过时的段落");
    await expect(page.getByText("正在编辑 README.md")).toBeInViewport();
    await expect(first).not.toBeInViewport();
    await page.getByRole("button", { name: "第 2 / 4 步" }).click();
    await expect(first).toBeInViewport();
    await expect(page.getByRole("button", { name: "回到底部" })).toBeVisible();
    await expect(page).toHaveScreenshot(`stream-plan-steps-${theme}.png`);
    expect(await page.evaluate(() => window.cupperViolations)).toEqual([]);
  });

  test(`UI-plan markdown ${theme}`, async ({ page }) => {
    await show(page, PLAN_MARKDOWN, theme);
    await page.getByRole("button", { name: "计划" }).click();
    await expect(page.getByText("先补测试")).toBeVisible();
    // A plan in a finished turn has no pill.
    await expect(page.getByRole("button", { name: /^第 / })).toHaveCount(0);
    await expect(page).toHaveScreenshot(`stream-plan-markdown-${theme}.png`);
    expect(await page.evaluate(() => window.cupperViolations)).toEqual([]);
  });

  test(`UI-error ${theme}`, async ({ page }) => {
    await show(page, ERRORS, theme);
    await expect(page.getByRole("button", { name: "重试" })).toBeVisible();
    await expect(page.getByRole("button", { name: "复制命令" })).toBeVisible();
    await expect(page).toHaveScreenshot(`stream-error-${theme}.png`);
    expect(await page.evaluate(() => window.cupperViolations)).toEqual([]);
  });
}
