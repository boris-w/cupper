// The new session page on the real app (design 5.7.4; AG-3, AG-4, CP-5;
// T144): the agent picked from both, and before the first message the
// options the host cached for it, probed on first use; what was picked is
// what the session is created with.

import { expect, test } from "./fixture";
import { events, texts, untilEvent } from "./helpers";
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
