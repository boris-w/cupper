import { expect, test, type Page } from "@playwright/test";

import { open, violations } from "./serve";

const file = (path: string, additions: number, deletions: number, change = "update") => ({
  ...{ path, additions, deletions },
  ...{ binary: false, change },
});
const diff = (diff_token: string, files: ReturnType<typeof file>[], caveats: string[] = []) => ({
  ...{ caveats, diff_token, files },
  stat: {
    added: files.reduce((sum, f) => sum + f.additions, 0),
    removed: files.reduce((sum, f) => sum + f.deletions, 0),
    files: files.length,
  },
});
const APPLY = "crates/host/src/git/apply.rs";
const FILES = [
  file(APPLY, 42, 7),
  file("packages/ui-core/src/review/ReviewPanel.tsx", 120, 0),
  file("README.md", 2, 2),
];
type Diffs = Record<string, ReturnType<typeof diff>>;
type Theme = "light" | "dark";
const DIFFS: Diffs = {
  last_turn: diff("dt_t", FILES.slice(0, 2), ["mixed_changes"]),
  unstaged: diff("dt_u", FILES),
  staged: diff("dt_s", FILES.slice(2)),
};
const EMPTY: Diffs = { ...DIFFS, unstaged: diff("dt_u", []) };
const TREE: Diffs = {
  ...DIFFS,
  unstaged: diff("dt_u", [
    ...FILES.slice(0, 1),
    file("crates/host/src/git/diff.rs", 3, 1),
    file("crates/host/src/git/a_rather_long_name_cut_in_the_middle_by_the_tree.rs", 10, 0, "add"),
    file("docs/old-notes.md", 0, 30, "delete"),
    ...FILES.slice(1),
  ]),
};

/** Session s_1 with the side panel open on `diffs`; `changed` paths make the configuration notice. */
async function show(
  page: Page,
  opts: { theme?: Theme; diffs?: Diffs; changed?: string[]; tree?: boolean; status?: object } = {},
) {
  const { theme = "light", diffs = DIFFS, changed, tree = false, status: extra = {} } = opts;
  const prefs = { "layout.sidePanelOpen": true, "layout.fileTreeOpen": tree };
  await open(page, { info: { theme, prefs } });
  await page.evaluate(
    ([diffs, changed, extra]) => {
      window.cupperHost.handle("git/diff", ({ params }) => ({
        result: diffs[(params as { scope: string }).scope],
      }));
      const status = { ahead: 0, behind: 0, files: [], repo_config_changed: changed, ...extra };
      window.cupperHost.handle("git/status", () => ({ result: status }));
      window.cupperShell.navigate({ kind: "session", session_id: "s_1" });
    },
    [diffs, changed, extra] as const,
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

for (const theme of ["light", "dark"] as const) {
  test(`UI-review-tree ${theme}: directories, status icons and a long name`, async ({ page }) => {
    const panel = await show(page, { theme, diffs: TREE, tree: true });
    await expect(panel.getByRole("navigation", { name: "文件" })).toBeVisible();
    await expect(panel).toHaveScreenshot(`review-tree-${theme}.png`);
    expect(await violations(page)).toEqual([]);
  });
}

test("UI-review-tree opens from the header and closes with ⌘⇧E", async ({ page }) => {
  const panel = await show(page, { diffs: TREE });
  const tree = panel.getByRole("navigation", { name: "文件" });
  await expect(tree).toBeHidden();
  await panel.getByRole("button", { name: "显示文件" }).click();
  await expect(tree).toBeVisible();
  await expect(panel.getByRole("button", { name: "隐藏文件" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await page.evaluate(() => window.cupperShell.command({ id: "toggle_file_tree" }));
  await expect(tree).toBeHidden();
});

test("UI-review-tree filter", async ({ page }) => {
  const panel = await show(page, { diffs: TREE, tree: true });
  const filter = panel.getByRole("textbox", { name: "筛选文件…" });
  await filter.fill("git/");
  await expect(panel.getByRole("button", { name: "README.md" })).toBeHidden();
  await page.mouse.move(400, 780);
  await expect(panel).toHaveScreenshot("review-tree-filter.png");
  await filter.fill("nothing like it");
  await expect(panel.getByText("没有匹配的文件")).toBeVisible();
});

test("UI-review-tree a file: the jump, its actions by scope, its right-click menu", async ({
  page,
}) => {
  const panel = await show(page, { diffs: TREE, tree: true });
  const sent = (method: string) =>
    page.evaluate(
      (method) =>
        window.cupperHost.requests.filter((r) => r.method === method).map((r) => r.params),
      method,
    );
  const apply = panel.getByRole("button", { name: "apply.rs" });
  await apply.click();
  await expect(apply).toHaveAttribute("aria-current", "true");

  // The menu copies the path, without the system clipboard, and opens the workspace.
  await page.evaluate(() => {
    const copied: string[] = ((window as unknown as { copied: string[] }).copied = []);
    const writeText = (text: string) => Promise.resolve(void copied.push(text));
    Object.defineProperty(navigator, "clipboard", { value: { writeText } });
  });
  await panel.getByRole("button", { name: "README.md" }).click({ button: "right" });
  await expect(page.getByRole("menuitem", { name: "在外部应用中打开" })).toBeVisible();
  await expect(panel).toHaveScreenshot("review-tree-menu.png");
  await page.getByRole("menuitem", { name: "复制路径" }).click();
  expect(await page.evaluate(() => (window as unknown as { copied: string[] }).copied)).toEqual([
    "README.md",
  ]);
  await panel.getByRole("button", { name: "README.md" }).click({ button: "right" });
  await page.getByRole("menuitem", { name: "在外部应用中打开" }).click();
  await expect
    .poll(() => sent("workspace/open"))
    .toEqual([{ session_id: "s_1", target: "editor", path: "README.md" }]);

  // Unstaged: stage and revert; a revert asks first and takes no focus.
  const row = apply.locator("..");
  await apply.hover();
  await row.getByRole("button", { name: "暂存" }).click();
  await expect
    .poll(() => sent("git/stage"))
    .toEqual([expect.objectContaining({ diff_token: "dt_u", target: { paths: [APPLY] } })]);
  await row.getByRole("button", { name: "还原" }).click();
  const confirm = panel.getByRole("alertdialog", { name: `是否还原 ${APPLY}？` });
  await expect(confirm).toBeVisible();
  expect(await confirm.evaluate((el) => el.contains(document.activeElement))).toBe(false);
  expect(await sent("git/revert")).toEqual([]);

  // Staged: unstage only.
  await expect(confirm).toHaveAttribute("data-ready", "true");
  await page.getByRole("button", { name: "取消" }).click();
  await expect(confirm).toBeHidden();
  await scope(page, "已暂存");
  const readme = panel.getByRole("button", { name: "README.md" });
  await readme.hover();
  const actions = readme.locator("..").getByRole("button");
  await expect(actions).toHaveCount(2);
  await expect(actions.nth(1)).toHaveAccessibleName("取消暂存");
});

const BRANCH = { branch: "cupper/fix-login", upstream: "origin/cupper/fix-login" };

async function commitDialog(page: Page) {
  await page.getByRole("button", { name: "Git 操作" }).click();
  await page.getByRole("menuitem", { name: "提交…" }).click();
  const dialog = page.getByRole("dialog", { name: "提交或推送" });
  await dialog.getByRole("textbox", { name: "提交信息" }).fill("修复登录时的空指针");
  return dialog;
}

for (const theme of ["light", "dark"] as const) {
  test(`UI-review-commit ${theme}: the dialog`, async ({ page }) => {
    await show(page, { theme, status: BRANCH });
    await commitDialog(page);
    await expect(page).toHaveScreenshot(`review-commit-${theme}.png`);
    expect(await violations(page)).toEqual([]);
  });
}

test("UI-review-commit with the configuration notice and a failure's guidance", async ({
  page,
}) => {
  await show(page, { status: BRANCH, changed: [".git/hooks/pre-commit"] });
  await page.evaluate(() => {
    const data = {
      code: "git_error",
      reason: "signing_failed",
      stderr: "error: gpg failed to sign the data",
    };
    const error = { code: -32000, message: "git_error/signing_failed", data };
    window.cupperHost.handle("git/commit", () => ({ error }));
  });
  const dialog = await commitDialog(page);
  await dialog.getByRole("button", { name: "继续" }).click();
  await dialog.getByText("git 的输出").click();
  await expect(dialog.getByText("error: gpg failed to sign the data")).toBeVisible();
  await page.mouse.move(400, 780);
  await expect(page).toHaveScreenshot("review-commit-failed.png");
});
