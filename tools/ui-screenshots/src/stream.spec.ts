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
      final_item_ids: ["i_2"],
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

/** The final answer is the messages after the last tool call (design 5.3.5), as the host fills it. */
const completed = (turn_id: string, status = "completed", final_item_ids: string[] = []) => ({
  type: "turn.completed",
  data: { turn_id, status, ended_at: 2, duration_ms: 8000, final_item_ids },
});

const HIGHLIGHT = [
  ...header,
  started("i_1", "agent_message"),
  delta("i_1", CODE),
  done("i_1"),
  completed("t_1", "completed", ["i_1"]),
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
  completed("t_1", "completed", ["i_2"]),
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

/** Turn `n`'s start and the user's message that opens it. */
const opens = (n: number, text: string) => [
  {
    type: "turn.started",
    data: { turn_id: `t_${n}`, started_at: STARTED, turn_no: n, origin: "user" },
  },
  started(`i_u${n}`, "user_message", {
    turn_id: `t_${n}`,
    content: [{ type: "text", text }],
    steered: false,
  }),
];
const in2 = { turn_id: "t_2" };
/**
 * Two completed turns: the first with work, a notice and a final answer; the
 * second ends on a tool call, so it has no final answer (T146, Q from the host's rule).
 */
const FOLD = [
  ...header,
  started("i_1", "thought"),
  done("i_1", { duration_ms: 2000 }),
  tool("i_2", "read", { paths: ["README.md"], group_id: "g_1" }),
  done("i_2"),
  tool("i_3", "read", { paths: ["package.json"], group_id: "g_1" }),
  done("i_3"),
  started("i_4", "notice", { code: "upstream_retry", params: { attempt: 2, max: 5 } }),
  tool("i_5", "edit", { paths: ["README.md"] }),
  done("i_5"),
  started("i_6", "agent_message"),
  delta("i_6", "README 的安装一节改好了，命令换成了 `pnpm i`。"),
  done("i_6"),
  completed("t_1", "completed", ["i_6"]),
  ...opens(2, "再跑一遍测试"),
  started("i_7", "agent_message", in2),
  delta("i_7", "我来跑一遍。"),
  done("i_7"),
  tool("i_8", "execute", { ...in2, command: "pnpm test" }),
  done("i_8", { exit_code: 0 }),
  completed("t_2", "completed", []),
] as EventBody[];
/**
 * A cancelled turn whose command ran on: its call, pushed out of view by the
 * work after it, and the late output in the next turn.
 */
const LATE = [
  ...header,
  tool("i_1", "execute", { command: "cargo test --workspace" }),
  done("i_1", { status: "abandoned", may_still_run: true }),
  ...Array.from({ length: 14 }, (_, n) => n + 1).flatMap((n) => [
    tool(`i_t${n}`, "read", { paths: [`docs/part-${n}.md`] }),
    done(`i_t${n}`),
  ]),
  completed("t_1", "cancelled"),
  ...opens(2, "测试跑完了吗？"),
  started("i_l", "late_tool_output", {
    ...in2,
    ref: { item_id: "i_1", turn_id: "t_1", title: "cargo test", command: "cargo test --workspace" },
    output_state: "complete",
    exit_code: 0,
  }),
  delta("i_l", "test result: ok. 412 passed; 0 failed\n", "output"),
  started("i_2", "agent_message", in2),
  delta("i_2", "跑完了，全部通过。"),
  done("i_2"),
] as EventBody[];

const snapshot = (turn_id: string, status: string, more: object = {}) => ({
  type: "snapshot.recorded",
  data: { turn_id, phase: "end", status, excluded: [], ...more },
});
const in3 = { turn_id: "t_3" };
/** Three finished turns: changes recorded, recorded in part, and not recorded at all. */
const CHANGES = [
  ...header,
  tool("i_1", "edit", { paths: ["README.md"] }),
  done("i_1"),
  tool("i_2", "edit", { paths: ["src/lib.rs"] }),
  done("i_2"),
  started("i_3", "agent_message"),
  delta("i_3", "README 和 `src/lib.rs` 都改好了。"),
  done("i_3"),
  completed("t_1", "completed", ["i_3"]),
  snapshot("t_1", "ok", { stat: { files: 2, added: 12, removed: 3 } }),
  ...opens(2, "把样例数据也更新一下"),
  started("i_4", "agent_message", in2),
  delta("i_4", "样例数据更新了。"),
  done("i_4"),
  completed("t_2", "completed", ["i_4"]),
  snapshot("t_2", "partial", {
    stat: { files: 1, added: 1, removed: 0 },
    excluded: [{ path: "data/sample.bin", reason: "too_large" }],
  }),
  ...opens(3, "再整理一下目录"),
  started("i_5", "agent_message", in3),
  delta("i_5", "目录整理好了。"),
  done("i_5"),
  completed("t_3", "completed", ["i_5"]),
  snapshot("t_3", "failed", { reason: "timeout" }),
] as EventBody[];
const file = (path: string, additions: number, deletions: number) => ({
  ...{ path, additions, deletions, binary: false, change: "update" },
});
/** Each turn's `git/diff`, by its `{turn}` scope. */
const TURN_DIFFS = {
  t_1: [file("README.md", 8, 3), file("src/lib.rs", 4, 0)],
  t_2: [file("data/sample.json", 1, 0)],
};

/**
 * The stream of `CHANGES`, its turns' diffs answered, and `revert` for
 * `git/revert`; the answers are set before the session page asks.
 */
async function showChanges(page: Page, theme: "light" | "dark", revert?: object) {
  await open(page, { info: { theme }, streams: { "session/s_1": CHANGES } });
  await page.evaluate(
    ([diffs, revert]) => {
      window.cupperHost.handle("git/diff", ({ params }) => {
        const scope = (params as { scope: string | { turn: string } }).scope;
        const files = typeof scope === "object" ? (diffs[scope.turn as "t_1"] ?? []) : [];
        const sum = (key: "additions" | "deletions") => files.reduce((n, f) => n + f[key], 0);
        const stat = { added: sum("additions"), removed: sum("deletions"), files: files.length };
        return { result: { caveats: [], diff_token: `dt_${JSON.stringify(scope)}`, files, stat } };
      });
      window.cupperHost.handle("git/revert", () => (revert ?? { result: {} }) as never);
      window.cupperShell.navigate({ kind: "session", session_id: "s_1" });
    },
    [TURN_DIFFS, revert] as const,
  );
}

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

for (const theme of ["light", "dark"] as const) {
  test(`UI-stream-fold ${theme}: completed turns fold to 用时, keeping what always shows`, async ({
    page,
  }) => {
    await show(page, FOLD, theme);
    const took = page.getByRole("button", { name: "用时 8 秒" });
    const [first, second] = [took.first(), took.last()];
    await expect(first).toHaveAttribute("aria-expanded", "false");
    await expect(second).toHaveAttribute("aria-expanded", "false");
    // The work folds away; the notice and the final answer stay; the second turn has none.
    await expect(page.getByText("读取了 2 个文件")).toBeHidden();
    await expect(page.getByText("上游服务暂时不可用，正在重试（第 2 / 5 次）。")).toBeVisible();
    await expect(page.getByText("README 的安装一节改好了")).toBeVisible();
    await expect(page.getByText("我来跑一遍。")).toBeHidden();
    await page.mouse.move(400, 780);
    await expect(page).toHaveScreenshot(`stream-fold-${theme}.png`);
    expect(await page.evaluate(() => window.cupperViolations)).toEqual([]);
  });
}

test("UI-stream-fold opens a turn and folds it again", async ({ page }) => {
  await show(page, FOLD, "light");
  const first = page.getByRole("button", { name: "用时 8 秒" }).first();
  await first.click();
  await expect(first).toHaveAttribute("aria-expanded", "true");
  await expect(page.getByText("读取了 2 个文件")).toBeVisible();
  await expect(page.getByText("已编辑 README.md")).toBeVisible();
  await page.mouse.move(400, 780);
  await expect(page).toHaveScreenshot("stream-fold-open.png");
  await first.click();
  await expect(page.getByText("已编辑 README.md")).toBeHidden();
});

test("UI-late-output folded, the jumps both ways and the final exit code", async ({ page }) => {
  await show(page, LATE, "light");
  const late = page.getByRole("button", { name: "取消后仍在运行：cargo test --workspace" });
  const original = page.getByText("命令可能仍在运行");
  await expect(late).toHaveAttribute("aria-expanded", "false");
  await expect(original).not.toBeInViewport();
  await page.getByRole("button", { name: "跳到原条目" }).click();
  await expect(original).toBeInViewport();
  await page.mouse.move(400, 780);
  await expect(page).toHaveScreenshot("stream-late-original.png");
  await page.getByRole("button", { name: "最终退出码 0" }).click();
  await expect(late).toBeInViewport();
  await late.click();
  await expect(page.getByText("test result: ok. 412 passed; 0 failed")).toBeVisible();
  await page.mouse.move(400, 780);
  await expect(page).toHaveScreenshot("stream-late-output.png");
});

for (const theme of ["light", "dark"] as const) {
  test(`UI-turn-changes ${theme}: the cards of folded turns, whole, in part and not recorded`, async ({
    page,
  }) => {
    await showChanges(page, theme);
    const cards = page.locator("[data-turn-changes]");
    await expect(cards).toHaveCount(3);
    await expect(cards.nth(0).getByText("README.md")).toBeVisible();
    await expect(cards.nth(0).getByRole("button", { name: "撤销" })).toBeVisible();
    await expect(page.getByText("有 1 个文件没有记录下来，不能撤销")).toBeVisible();
    await expect(page.getByText("这一轮的改动没能记录")).toBeVisible();
    await expect(page.getByRole("button", { name: "撤销" })).toHaveCount(1);
    await page.mouse.move(400, 780);
    await expect(page).toHaveScreenshot(`stream-changes-${theme}.png`);
    expect(await page.evaluate(() => window.cupperViolations)).toEqual([]);
  });
}

test("UI-turn-changes a file opens the review panel on its turn", async ({ page }) => {
  await showChanges(page, "light");
  await page
    .locator("[data-turn-changes]")
    .first()
    .getByRole("button", { name: /src\/lib\.rs/ })
    .click();
  const panel = page.locator('aside[aria-label="侧边面板"]');
  await expect(panel.getByRole("button", { name: "查阅来源" })).toHaveText(/第 1 轮/);
  await expect(panel.getByText("src/lib.rs")).toBeVisible();
  await page.mouse.move(400, 780);
  await expect(page).toHaveScreenshot("stream-changes-review.png");
});

test("UI-turn-changes undo asks first; a stale patch says so and leads to the review panel", async ({
  page,
}) => {
  const stale = {
    error: {
      code: -32000,
      message: "conflict/stale_diff",
      data: { code: "conflict", reason: "stale_diff" },
    },
  };
  await showChanges(page, "light", stale);
  const card = page.locator("[data-turn-changes]").first();
  await card.getByRole("button", { name: "撤销" }).click();
  const confirm = card.getByRole("alertdialog", { name: "是否还原更改？" });
  await expect(confirm).toHaveAttribute("data-ready", "true");
  await expect(confirm.getByRole("button", { name: "确认" })).toBeFocused();
  await card.scrollIntoViewIfNeeded();
  await page.mouse.move(400, 780);
  await expect(page).toHaveScreenshot("stream-changes-undo.png");
  await confirm.getByRole("button", { name: "确认" }).click();
  await expect(card.getByText("之后已有新的改动，无法自动撤销")).toBeVisible();
  await card.getByRole("button", { name: "打开审查面板" }).click();
  const panel = page.locator('aside[aria-label="侧边面板"]');
  await expect(panel.getByRole("button", { name: "查阅来源" })).toHaveText(/第 1 轮/);
});
