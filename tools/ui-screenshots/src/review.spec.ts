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

type File = ReturnType<typeof file>;
const lines = (n: number, sign: string, text: (i: number) => string) =>
  Array.from({ length: n }, (_, i) => `${sign}${text(i)}`);
/** What `git/file_diff` gives for `f`: one unit, its lines by the file's counts. */
function fileDiff(f: File, text = (i: number) => `    let value_${i} = compute(${i});`) {
  const [added, removed] = [lines(f.additions, "+", text), lines(f.deletions, "-", text)];
  const hunk = (unit: string, header: string, body: string[]) => ({
    path: f.path,
    hunks: [{ hunk_id: `hk_${f.path.length}`, unit, header, lines: body }],
  });
  if (f.change === "add") {
    return hunk("whole_file", `new file mode 100644\n@@ -0,0 +1,${f.additions} @@`, added);
  }
  if (f.change === "delete") {
    return hunk("whole_file", `deleted file mode 100644\n@@ -1,${f.deletions} +0,0 @@`, removed);
  }
  const header = `@@ -10,${f.deletions + 2} +10,${f.additions + 2} @@ fn apply()`;
  return hunk("hunk", header, [" fn apply() {", ...removed, ...added, " }"]);
}

/** The review document's frame in the page, to locate in and to evaluate in. */
const reviewFrame = (page: Page) => page.frameLocator('iframe[title="变更详情"]');
const reviewDocument = (page: Page) =>
  page.frames().find((frame) => frame.url().startsWith("https://cupper-review.test/"));

/** Session s_1 with the side panel open on `diffs`; `changed` paths make the configuration notice. */
async function show(
  page: Page,
  opts: {
    theme?: "light" | "dark";
    diffs?: Diffs;
    changed?: string[];
    tree?: boolean;
    text?: (i: number) => string;
  } = {},
) {
  const { theme = "light", diffs = DIFFS, changed, tree = false } = opts;
  const prefs = { "layout.sidePanelOpen": true, "layout.fileTreeOpen": tree };
  const files = Object.values(diffs).flatMap((d) => d.files);
  const hunks = Object.fromEntries(files.map((f) => [f.path, fileDiff(f, opts.text)]));
  await open(page, { info: { theme, prefs } });
  await page.evaluate(
    ([diffs, changed, hunks]) => {
      window.cupperHost.handle("git/diff", ({ params }) => ({
        result: diffs[(params as { scope: string }).scope],
      }));
      const status = { ahead: 0, behind: 0, files: [], repo_config_changed: changed };
      window.cupperHost.handle("git/status", () => ({ result: status }));
      window.cupperHost.handle("git/file_diff", ({ params }) => ({
        result: hunks[(params as { path: string }).path],
      }));
      window.cupperShell.navigate({ kind: "session", session_id: "s_1" });
    },
    [diffs, changed, hunks] as const,
  );
  // By its label, as an open menu hides the rest of the page from the accessibility tree.
  const panel = page.locator('aside[aria-label="侧边面板"]');
  await expect(panel.getByRole("button", { name: "查阅来源" })).toBeVisible();
  if (diffs.unstaged?.files.length) {
    await expect(reviewFrame(page).locator("[data-line]").first()).toBeVisible();
  }
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
    .toEqual([{ session_id: "s_1", target: "editor" }]);

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

const MARKUP = [
  "<style data-injected>body { display: none }</style>",
  "<img data-injected src=x onerror=alert(1)>",
  '<a data-injected href="https://example.com/">a</a>',
  '<base data-injected href="https://example.com/">',
  '<link data-injected rel="stylesheet" href="https://example.com/x.css">',
  '<meta data-injected http-equiv="refresh" content="0">',
];

test("UNIT-review-injection: markup in a file's name and lines shows as text", async ({ page }) => {
  const name = "docs/<img data-injected src=x>.md";
  const unstaged = diff("dt_u", [file(name, MARKUP.length, 0, "add")]);
  await show(page, { diffs: { ...DIFFS, unstaged }, text: (i) => MARKUP[i] ?? "" });
  const review = page
    .frames()
    .find((frame) => frame.url().startsWith("https://cupper-review.test/"));
  await expect(reviewFrame(page).getByText(name)).toBeVisible();
  const found = await review?.evaluate(() => {
    const all: Element[] = [];
    const walk = (root: Document | ShadowRoot) =>
      root.querySelectorAll("*").forEach((el) => {
        all.push(el);
        if (el.shadowRoot) walk(el.shadowRoot);
      });
    walk(document);
    const shown = all.filter((el) => el.hasAttribute("data-line")).map((el) => el.textContent);
    return { injected: all.filter((el) => el.hasAttribute("data-injected")).length, shown };
  });
  expect(found?.injected).toBe(0);
  expect(found?.shown).toEqual(MARKUP);
  expect(await page.locator("[data-injected]").count()).toBe(0);
  expect(await violations(page)).toEqual([]);
});

test("UI-review-panel the review document: the fold, the jump, keys and the theme", async ({
  page,
}) => {
  const panel = await show(page, { diffs: TREE, tree: true });
  const review = reviewFrame(page);
  // A fold goes up to the panel and comes back down as the file's state.
  await review.getByRole("button", { name: "收起" }).first().click();
  await expect(review.getByRole("button", { name: "展开" })).toHaveCount(1);
  await page.getByRole("button", { name: "选项" }).click();
  await page.getByRole("menuitem", { name: "收起全部" }).click();
  await expect(review.getByRole("button", { name: "收起" })).toHaveCount(0);

  // The tree's jump scrolls the review document to the file.
  await page.getByRole("button", { name: "选项" }).click();
  await page.getByRole("menuitem", { name: "展开全部" }).click();
  await panel.getByRole("button", { name: "README.md" }).click();
  await expect(review.getByTitle("README.md", { exact: true })).toBeInViewport();

  // Its keys reach the main document, which owns the shortcuts.
  await page.evaluate(() => {
    const keys: string[] = ((window as unknown as { keys: string[] }).keys = []);
    document.addEventListener("keydown", (event) => keys.push(event.key));
  });
  await review.getByRole("button", { name: "收起" }).first().press("k");
  const keys = () => page.evaluate(() => (window as unknown as { keys: string[] }).keys);
  await expect.poll(keys).toEqual(["k"]);

  // The theme follows the main document's.
  const theme = () => reviewDocument(page)?.evaluate(() => document.documentElement.dataset.theme);
  await page.evaluate(() => window.cupperShell.setTheme("dark"));
  await expect.poll(theme).toBe("dark");
});
