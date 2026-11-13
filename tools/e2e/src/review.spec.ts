// The review document in the window (design 5.7.5, 5.7.8): its own origin,
// its one port, markup as text, and what it says checked by the main document;
// its comments, its reverts and its copies.

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import type { Frame } from "@playwright/test";

import { expect, test, type Cupper } from "./fixture";
import { texts, untilEvent } from "./helpers";
import { focusWindow } from "./input";
import { openSession, session } from "./nfr1";

declare global {
  interface Window {
    /** e2e builds only (`apps/desktop/src/renderer/review.ts`). */
    __cupperReview: {
      send(message: unknown): void;
      token(): string;
      copy(text: string): Promise<string>;
    };
    __violations: string[];
  }
}

const MARKUP = [
  "<style data-injected>body { display: none }</style>",
  "<img data-injected src=x onerror=alert(1)>",
  '<a data-injected href="https://example.com/">a</a>',
  '<base data-injected href="https://example.com/">',
  '<link data-injected rel="stylesheet" href="https://example.com/x.css">',
  '<meta data-injected http-equiv="refresh" content="0">',
];

/** The review document's frame, once the window has it. */
async function reviewDocument(cupper: Cupper): Promise<Frame> {
  let found: Frame | undefined;
  const frame = () =>
    cupper.window.frames().find((frame) => frame.url() === "app://cupper-review/review.html");
  await expect.poll(() => (found = frame()) !== undefined).toBe(true);
  return found as Frame;
}

/** The elements of the document and its shadow roots that match `selector`. */
const matching = (selector: string) => {
  const found: Element[] = [];
  const walk = (root: Document | ShadowRoot) =>
    root.querySelectorAll("*").forEach((el) => {
      if (el.matches(selector)) found.push(el);
      if (el.shadowRoot) walk(el.shadowRoot);
    });
  walk(document);
  return found.map((el) => el.textContent);
};

test(
  "E2E-SEC-review-iso: its own origin and port; markup as text; stray messages dropped; its revert only asks",
  { tag: "@smoke" },
  async ({ cupper }) => {
    await cupper.launch();
    await cupper.window.addInitScript(() => {
      window.__violations = [];
      document.addEventListener("securitypolicyviolation", (event) =>
        window.__violations.push(`${event.effectiveDirective} ${event.blockedURI}`),
      );
    });
    await cupper.window.reload();
    await cupper.connected();
    const stream = await session(cupper, "say ready");
    await untilEvent(cupper, stream, (event) => event.type === "turn.completed");
    await openSession(cupper, "say ready");
    const readme = join(cupper.repo, "README.md");
    const changed = ["e2e", ...MARKUP, ""].join("\n");
    writeFileSync(readme, changed);
    const t = await texts(cupper);
    await cupper.window.getByRole("button", { name: t("shell:toggle_side_panel") }).click();
    const review = await reviewDocument(cupper);
    const lines = () => review.evaluate(matching, "[data-line]");
    await expect.poll(lines).toEqual(["e2e", ...MARKUP]);

    // Two origins: the review document cannot reach the main document.
    expect(await cupper.window.evaluate(() => location.origin)).toBe("app://cupper");
    expect(await review.evaluate(() => location.origin)).toBe("app://cupper-review");
    const parent = await review.evaluate(() => {
      try {
        return window.parent.document.title;
      } catch (error) {
        return (error as Error).name;
      }
    });
    expect(parent).toBe("SecurityError");
    // The markup is text in both documents.
    expect(await review.evaluate(matching, "[data-injected]")).toEqual([]);
    expect(await cupper.window.evaluate(matching, "[data-injected]")).toEqual([]);

    // To the review document: a window message and a second port, both dropped.
    const theme = () => document.documentElement.dataset.theme;
    await cupper.window.evaluate(() => {
      const frame = document.querySelector("iframe")?.contentWindow;
      const theme = document.documentElement.dataset.theme === "dark" ? "light" : "dark";
      const file = { path: "stray.md", additions: 1, deletions: 0, binary: false, change: "add" };
      const files = { type: "files", diff_token: "dt_stray", files: [file], collapsed: [] };
      const stray = { ...files, view: "auto", wrap: false };
      frame?.postMessage(stray, "*");
      const { port1, port2 } = new MessageChannel();
      const init = { type: "init", v: 1, theme, locale: "en", strings: {} };
      frame?.postMessage(init, "*", [port2]);
      port1.postMessage(stray);
    });
    // To the main document: a revert on the window, one with a stale token, a
    // stage of a hunk it never fetched; then a fold, which it does carry out.
    const token = await review.evaluate(() => window.__cupperReview.token());
    const revert = { type: "hunk_action", action: "revert", path: "README.md" };
    await review.evaluate(
      ([revert, token]) => {
        window.parent.postMessage({ ...revert, diff_token: token }, "*");
        window.__cupperReview.send({ ...revert, diff_token: "dt_stale" });
        const hunk_ids = ["hk_000000000000000000000000"];
        window.__cupperReview.send({ ...revert, action: "stage", diff_token: token, hunk_ids });
        window.__cupperReview.send({ type: "set_collapsed", path: "README.md", collapsed: true });
      },
      [revert, token] as const,
    );
    await expect(review.getByRole("button", { name: t("review:doc_unfold") })).toBeVisible();
    const confirm = cupper.window.getByRole("alertdialog");
    await expect(confirm).toHaveCount(0);
    await expect(review.getByText("stray.md")).toHaveCount(0);
    expect(await review.evaluate(theme)).toBe(await cupper.window.evaluate(theme));

    // A revert on its port opens the confirmation and nothing more.
    await review.evaluate(
      ([revert, token]) => window.__cupperReview.send({ ...revert, diff_token: token }),
      [revert, token] as const,
    );
    await expect(confirm).toHaveAccessibleName(
      t("review:revert_title_path", { path: "README.md" }),
    );
    await expect(confirm).toHaveAttribute("data-ready", "true");
    await confirm.getByRole("button", { name: t("review:cancel") }).click();
    await expect(confirm).toHaveCount(0);
    const staged = execFileSync("git", ["diff", "--cached", "--name-only"], { cwd: cupper.repo });
    expect(staged.toString()).toBe("");
    expect(readFileSync(readme, "utf8")).toBe(changed);

    // The page's zoom reaches the review document, with no message of its own.
    const ratio = () => window.devicePixelRatio;
    const zoomed = (await cupper.window.evaluate(ratio)) * 1.25;
    await cupper.app.evaluate(({ BrowserWindow }) =>
      BrowserWindow.getAllWindows()[0]?.webContents.setZoomFactor(1.25),
    );
    await expect.poll(() => cupper.window.evaluate(ratio)).toBe(zoomed);
    await expect.poll(() => review.evaluate(ratio)).toBe(zoomed);

    expect(await cupper.window.evaluate(() => window.__violations)).toEqual([]);
    expect(await review.evaluate(() => window.__violations)).toEqual([]);
  },
);

/** A session on screen with README.md changed, the side panel open on it; its stream and the review document. */
async function reviewing(cupper: Cupper) {
  await cupper.launch();
  await cupper.connected();
  const stream = await session(cupper, "say ready");
  await untilEvent(cupper, stream, (event) => event.type === "turn.completed");
  await openSession(cupper, "say ready");
  writeFileSync(join(cupper.repo, "README.md"), "e2e\nchanged\n");
  const t = await texts(cupper);
  await cupper.window.getByRole("button", { name: t("shell:toggle_side_panel") }).click();
  const review = await reviewDocument(cupper);
  // pierre draws the lines as plain text, then again once highlighted, which
  // replaces them, and the "+" of a line hovered before (T260). Highlighted,
  // their tokens have colours.
  await expect(review.locator("[data-line] span[style]").first()).toBeVisible();
  return { t, stream, review };
}

test("E2E-review-comment: a draft from the review document goes with the next message", async ({
  cupper,
}) => {
  const { t, stream, review } = await reviewing(cupper);
  await review.locator("[data-line]").nth(1).hover();
  await review.locator("[data-utility-button]").click();
  const box = review.getByRole("textbox", { name: t("review:doc_comment") });
  await box.fill("why this line");
  await box.press("Meta+Enter");
  await expect(review.getByText("why this line")).toBeVisible();
  await expect(
    cupper.window.getByText(t("composer:attachment_comments", { count: 1 })),
  ).toBeVisible();

  const composer = cupper.window.locator("[data-composer] textarea");
  await composer.fill("say noted");
  await composer.press("Enter");
  const shown = t("stream:comments", { count: 1 });
  await expect(cupper.window.getByRole("main").getByText(shown)).toBeVisible();
  // As the host stored it: the comment's block, with its file.
  const sent = await untilEvent(cupper, stream, (event) =>
    JSON.stringify(event.data).includes("why this line"),
  );
  expect(JSON.stringify(sent.data)).toContain('"path":"README.md"');
});

test("E2E-SEC-revert-confirm: the review document's revert takes no focus and one waits at a time; the button's focuses 确认", async ({
  cupper,
}) => {
  const { t, review } = await reviewing(cupper);
  const revert = review.getByRole("button", { name: t("review:action_revert") });
  await revert.first().click();
  const named = t("review:revert_title_path", { path: "README.md" });
  const confirm = cupper.window.getByRole("alertdialog");
  await expect(confirm).toHaveAccessibleName(named);
  expect(await confirm.evaluate((el) => el.contains(document.activeElement))).toBe(false);
  // A hunk's revert while one waits: ignored.
  await revert.nth(1).click();
  await expect(confirm).toHaveCount(1);
  await expect(confirm).toHaveAccessibleName(named);
  await expect(confirm).toHaveAttribute("data-ready", "true");
  await confirm.getByRole("button", { name: t("review:cancel") }).click();
  await expect(confirm).toHaveCount(0);

  await cupper.window.getByRole("button", { name: t("review:revert_all") }).click();
  await expect(confirm).toHaveAccessibleName(t("review:revert_title"));
  await expect(confirm.getByRole("button", { name: t("review:confirm") })).toBeFocused();
  await expect(confirm).toHaveAttribute("data-ready", "true");
  await confirm.getByRole("button", { name: t("review:cancel") }).click();
  expect(readFileSync(join(cupper.repo, "README.md"), "utf8")).toBe("e2e\nchanged\n");
});

// The system clipboard is the user's: only the runner's is written (T104).
test(
  "E2E-review-comment: a copy in the review document reaches the clipboard, with the focus there only",
  { tag: "@focus" },
  async ({ cupper }) => {
    test.skip(process.env.GITHUB_ACTIONS !== "true", "writes the system clipboard: runner only");
    const { review } = await reviewing(cupper);
    await focusWindow(cupper);
    await cupper.window.locator("[data-composer] textarea").focus();
    expect(await review.evaluate(() => window.__cupperReview.copy("t238 refused"))).toBe(
      "NotAllowedError",
    );
    await review.locator("[data-line]").first().click();
    expect(await review.evaluate(() => window.__cupperReview.copy("t238 copied"))).toBe("ok");
    const read = () => cupper.app.evaluate(({ clipboard }) => clipboard.readText());
    await expect.poll(read).toBe("t238 copied");
    console.log(`clipboard read back: ${JSON.stringify(await read())}`);
  },
);

test("E2E-S3: the agent's change reviewed, commented on, staged by hunk and committed", async ({
  cupper,
}) => {
  const git = (...args: string[]) =>
    execFileSync("git", ["-C", cupper.repo, ...args], { encoding: "utf8" });
  // Who commits, as a user's repository would say; and a file with room for two hunks.
  git("config", "user.name", "cupper-e2e");
  git("config", "user.email", "e2e@cupper.invalid");
  git("config", "commit.gpgsign", "false");
  const lines = Array.from({ length: 20 }, (_, i) => `line ${i + 1}`);
  writeFileSync(join(cupper.repo, "notes.txt"), `${lines.join("\n")}\n`);
  git("add", "notes.txt");
  git("commit", "--quiet", "--message", "notes");
  const changed = lines.map((line, i) => (i === 1 || i === 18 ? `${line} changed` : line));

  await cupper.launch();
  await cupper.connected();
  const t = await texts(cupper);
  const script = `write_file notes.txt ${changed.join("\\n")}\\n; say written`;
  const stream = await session(cupper, script);
  await untilEvent(cupper, stream, (event) => event.type === "turn.completed");
  await cupper.window.getByRole("button", { name: script.slice(0, 12) }).click();
  await cupper.window.getByRole("button", { name: t("shell:toggle_side_panel") }).click();
  const review = await reviewDocument(cupper);
  await expect(review.getByText("line 19 changed")).toBeVisible();

  // A comment on a line rides the next message.
  await review.locator("[data-line]").nth(1).hover();
  await review.locator("[data-utility-button]").click();
  const box = review.getByRole("textbox", { name: t("review:doc_comment") });
  await box.fill("name it better");
  await box.press("Meta+Enter");
  const main = cupper.window.getByRole("main");
  await expect(
    cupper.window.getByText(t("composer:attachment_comments", { count: 1 })),
  ).toBeVisible();
  const composer = cupper.window.locator("[data-composer] textarea");
  await composer.fill("say noted");
  await composer.press("Enter");
  const sent = await untilEvent(cupper, stream, (event) =>
    JSON.stringify(event.data).includes("name it better"),
  );
  expect(JSON.stringify(sent.data)).toContain('"path":"notes.txt"');
  await expect(main.getByText(t("stream:comments", { count: 1 }))).toBeVisible();
  // Its turn over (the agent says the comment back with "noted"), the panel shows the repository as it stays.
  await untilEvent(
    cupper,
    stream,
    (event) => event.type === "turn.completed" && event.seq > sent.seq,
  );

  // The first hunk only: the file's own 暂存 comes first, then one under each hunk.
  const stage = review.getByRole("button", { name: t("review:action_stage") });
  await expect(stage).toHaveCount(3);
  await stage.nth(1).click();
  const staged = () => git("diff", "--cached");
  await expect.poll(staged).toContain("line 2 changed");
  expect(staged()).not.toContain("line 19 changed");

  await cupper.window.getByRole("button", { name: t("review:git_actions") }).click();
  await cupper.window.getByRole("menuitem", { name: t("review:git_commit_menu") }).click();
  await expect(cupper.window.getByRole("menu")).toHaveCount(0);
  const dialog = cupper.window.getByRole("dialog");
  await dialog.getByRole("textbox", { name: t("review:git_message") }).fill("one hunk");
  await dialog.getByRole("button", { name: t("review:git_continue") }).click();
  await expect.poll(() => git("log", "-1", "--format=%s").trim()).toBe("one hunk");
  const committed = git("show", "HEAD");
  expect(committed).toContain("line 2 changed");
  expect(committed).not.toContain("line 19 changed");
  expect(git("diff")).toContain("line 19 changed");
});
