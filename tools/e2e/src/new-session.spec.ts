// The new session page on the real app (design 5.7.4; AG-3, AG-4, CP-5;
// T144): the agent picked from both, and before the first message the
// options the host cached for it, probed on first use; what was picked is
// what the session is created with. E2E-S1 (PRD S1, T170): a Claude and a
// Codex session at once, each in a worktree of its own.

import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "./fixture";
import { events, texts, untilEvent } from "./helpers";
import { focusWindow, shortcut } from "./input";
import { asked } from "./nfr1";
import { BUILT } from "./paths";

test.describe("with Codex too", () => {
  const program = BUILT.fakeAgent;
  test.use({
    testConfig: {
      agents: { claude: { program }, codex: { program, args: ["--persona", "codex"] } },
    },
  });

  test(
    "E2E-new-session-agent-codex: Codex picked on the new session page makes a Codex session",
    { tag: "@smoke" },
    async ({ cupper }) => {
      await cupper.launch();
      await cupper.connected();
      const t = await texts(cupper);
      const window = cupper.window;
      // The project, by a first session; with no agent configuration it is trusted, as Codex needs.
      await cupper.cli.ok(["new", "--agent", "claude", "--project", cupper.repo, "say one"]);
      await expect(window.getByRole("heading", { level: 1 })).toHaveText(/repo/);

      const agent = window.getByRole("button", { name: t("composer:pick_agent") });
      await agent.click();
      const items = window.getByRole("menuitemradio");
      await expect(items).toHaveText([t("sidebar:agent_claude"), t("sidebar:agent_codex")]);
      await items.filter({ hasText: t("sidebar:agent_codex") }).click();
      await expect(agent).toHaveText(t("sidebar:agent_codex"));

      const composer = window.locator("[data-composer] textarea");
      await composer.fill("say from codex");
      await composer.press("Enter");
      await expect(window.getByRole("main").getByText("from codex", { exact: true })).toBeVisible();
      const sessions = async () => (await cupper.cli.run(["sessions"])).lines;
      await expect
        .poll(async () => (await sessions()).find((line) => line.title === "say from codex"))
        .toMatchObject({ agent: "codex", workspace_kind: "local" });
    },
  );

  test(
    "E2E-S1: Claude and Codex at once, each in its worktree; the sidebar, ⌘⌥A and the read marks follow",
    { tag: "@focus" },
    async ({ cupper }) => {
      await cupper.launch();
      await cupper.connected();
      const t = await texts(cupper);
      const window = cupper.window;
      await focusWindow(cupper);
      // The project, added from the sidebar; the new session page takes it.
      await window.getByRole("button", { name: t("sidebar:add_project") }).click();
      await cupper.dialogs.choose((await cupper.dialogs.next()).index, [cupper.repo]);
      const project = window.getByRole("region", { name: "repo" });
      await expect(window.getByRole("heading", { level: 1 })).toHaveText(/repo/);
      const row = (title: string) => project.getByRole("button", { name: new RegExp(title) });
      const newSession = window
        .getByRole("navigation", { name: t("shell:sidebar") })
        .getByRole("button", { name: t("sidebar:new_session"), exact: true });

      /** A session from the new session page: `agent`, a new worktree, `script`; its stream. */
      const start = async (agent: "claude" | "codex", script: string) => {
        const pick = async (menu: string, item: string) => {
          await window.getByRole("button", { name: menu }).click();
          await window.getByRole("menuitemradio", { name: item }).click();
          await expect(window.getByRole("menu")).toHaveCount(0);
        };
        await pick(t("composer:pick_agent"), t(`sidebar:agent_${agent}`));
        await pick(t("composer:pick_workspace"), t("composer:workspace_worktree"));
        // A worktree waits for its base: sending is blocked until the branches came.
        await expect(window.getByRole("button", { name: t("composer:pick_branch") })).toHaveText(
          "main",
        );
        const composer = window.locator("[data-composer] textarea");
        await composer.fill(script);
        await composer.press("Enter");
        let line: { session_id?: unknown } | undefined;
        const lines = async () => (await cupper.cli.run(["sessions"])).lines;
        await expect
          .poll(async () => (line = (await lines()).find((l) => l.title === script)))
          .toMatchObject({ agent, workspace_kind: "worktree" });
        return `session/${String(line?.session_id)}`;
      };

      // Claude waits for an approval; Codex runs meanwhile (the agent's own sleep).
      const fix = "write_file fix.txt fixed\\n; permission k1; say fixed";
      const claude = await start("claude", fix);
      await expect(row("fix.txt")).toContainText(t("sidebar:awaiting_approval"));
      await newSession.click();
      const tests = "write_file tests.txt tested\\n; sleep 10000; say tested";
      const codex = await start("codex", tests);
      await expect(row("tests.txt").getByRole("img", { name: t("sidebar:running") })).toBeVisible();
      await expect(row("fix.txt")).toContainText(t("sidebar:awaiting_approval"));
      // ⌘⌥A goes to the one that waits; answered from elsewhere, both end unread.
      await shortcut(cupper, "Command+Alt+A");
      await expect(row("fix.txt")).toHaveAttribute("aria-current", "page");
      await newSession.click();
      await cupper.cli.ok(["approve", claude.replace("session/", ""), await asked(cupper, claude)]);
      const unread = (title: string) => row(title).getByRole("img", { name: t("sidebar:unread") });
      await expect(unread("fix.txt")).toBeVisible();
      await expect(unread("tests.txt")).toBeVisible();
      // Opened in front, Codex's is read; Claude's stays unread.
      await row("tests.txt").click();
      await expect(unread("tests.txt")).toHaveCount(0);
      await expect(unread("fix.txt")).toBeVisible();

      // Each change in its own worktree; the main working tree is clean.
      const worktree = async (stream: string) =>
        String((await untilEvent(cupper, stream, (e) => e.type === "workspace.ready")).data.path);
      const [fixed, tested] = [await worktree(claude), await worktree(codex)];
      expect([fixed, tested].map((dir) => existsSync(join(dir, "fix.txt")))).toEqual([true, false]);
      expect([fixed, tested].map((dir) => existsSync(join(dir, "tests.txt")))).toEqual([
        false,
        true,
      ]);
      const status = execFileSync("git", ["-C", cupper.repo, "status", "--porcelain"], {
        encoding: "utf8",
      });
      expect(status).toBe("");
    },
  );
});

test("E2E-new-session-before-first-message: the cached options show, and the session takes those picked", async ({
  cupper,
}) => {
  await cupper.launch();
  await cupper.connected();
  const t = await texts(cupper);
  const window = cupper.window;
  await cupper.cli.ok(["new", "--agent", "claude", "--project", cupper.repo, "say one"]);

  // Before any message here: the agent's modes, models and commands.
  const permission = window.getByRole("button", { name: t("composer:permission") });
  await expect(permission).toHaveText("Manual");
  await expect(window.getByRole("button", { name: t("composer:model") })).toBeVisible();
  const composer = window.locator("[data-composer] textarea");
  await composer.fill("/");
  const slash = window.getByRole("listbox", { name: t("composer:slash_title") });
  await expect(slash.getByRole("option", { name: /\/compact/ })).toBeVisible();
  await composer.press("Escape");

  // The low-interruption tier says so; a new worktree from main.
  await permission.click();
  await window.getByRole("menuitemradio", { name: /^Accept edits/ }).click();
  await expect(window.getByText(t("composer:elevated_on"))).toBeVisible();
  await window.getByRole("button", { name: t("composer:pick_workspace") }).click();
  await window.getByRole("menuitemradio", { name: t("composer:workspace_worktree") }).click();
  await expect(window.getByRole("button", { name: t("composer:pick_branch") })).toHaveText("main");

  await composer.fill("say in the worktree");
  await composer.press("Enter");
  await expect(
    window.getByRole("main").getByText("in the worktree", { exact: true }),
  ).toBeVisible();
  const sessions = async () => (await cupper.cli.run(["sessions"])).lines;
  let created: { session_id?: unknown } | undefined;
  await expect
    .poll(async () => (created = (await sessions()).find((l) => l.title === "say in the worktree")))
    .toMatchObject({ agent: "claude", workspace_kind: "worktree" });
  const stream = `session/${String(created?.session_id)}`;
  const ready = await untilEvent(cupper, stream, (e) => e.type === "workspace.ready");
  expect(ready.data.branch).toMatch(/^cupper\//);
  const configured = events(cupper, stream).filter((e) => e.type === "session.configured");
  expect(configured.at(-1)?.data).toMatchObject({ config: { permission_mode: "acceptEdits" } });
});
