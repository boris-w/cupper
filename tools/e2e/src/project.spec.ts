// Projects and new sessions on the real app (design 5.7.4, 5.2.6; T081,
// T085, T135, T137): adding a project picks its folder in the native open
// dialog and asks about trusting its agent configuration in a native dialog
// that names it; the new session page creates a session with the project,
// agent and workspace picked.

import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "./fixture";
import { events, texts } from "./helpers";

test(
  "E2E-project-add-trust: added from the sidebar, trusted in the native dialog that names it",
  { tag: ["@focus", "@smoke"] },
  async ({ cupper }) => {
    // Agent configuration in the project: adding it asks about trust.
    mkdirSync(join(cupper.repo, ".claude/commands"), { recursive: true });
    writeFileSync(join(cupper.repo, ".claude/commands/hello.md"), "Say hello\n");
    await cupper.launch();
    await cupper.connected();
    const t = await texts(cupper);
    const named = t("privileged:project_path", { path: cupper.repo });

    await cupper.window.getByRole("button", { name: t("sidebar:add_project") }).click();
    const open = await cupper.dialogs.next(0);
    expect(open).toMatchObject({ kind: "open" });
    expect(open.properties).toContain("openDirectory");
    await cupper.dialogs.choose(open.index, [cupper.repo]);
    const asked = await cupper.dialogs.next(1);
    expect(asked).toMatchObject({
      kind: "message",
      message: t("privileged:trust_title"),
      buttons: [t("privileged:trust"), t("privileged:untrust")],
      defaultId: 1,
      cancelId: 1,
    });
    expect(asked.detail).toContain(named);
    expect(asked.detail).toContain(".claude/commands/hello.md");
    // Not trusted: added all the same, and marked.
    await cupper.dialogs.answer(asked.index, 1);
    const project = cupper.window.getByRole("region", { name: "repo" });
    await expect(project.getByText(t("sidebar:untrusted"))).toBeVisible();

    // A session there says so and offers to trust it, through the same dialog.
    await cupper.cli.ok(["new", "--agent", "claude", "--project", cupper.repo, "say hi"]);
    await project.getByRole("button", { name: /say hi/ }).click();
    await expect(cupper.window.getByText(t("stream:untrusted"))).toBeVisible();
    await cupper.window.getByRole("button", { name: t("stream:trust") }).click();
    const trust = await cupper.dialogs.next(2);
    expect(trust).toMatchObject({ message: t("privileged:trust_title") });
    expect(trust.detail).toContain(named);
    await cupper.dialogs.answer(trust.index, 0);
    await expect(project.getByText(t("sidebar:untrusted"))).toHaveCount(0);
    await expect(cupper.window.getByText(t("stream:untrusted"))).toHaveCount(0);
    const upserted = events(cupper, "index").filter((e) => e.type === "project.upserted");
    expect(upserted.at(-1)?.data).toMatchObject({ path: cupper.repo, trust_state: "trusted" });
  },
);

test(
  "E2E-new-session-agent-claude: the project, Claude and local picked make the session",
  { tag: "@smoke" },
  async ({ cupper }) => {
    const other = join(cupper.root, "other");
    mkdirSync(other);
    await cupper.launch();
    await cupper.connected();
    const t = await texts(cupper);
    const window = cupper.window;
    const project = ["--agent", "claude", "--project"];
    await cupper.cli.ok(["new", ...project, cupper.repo, "say one"]);
    const { session_id: second } = await cupper.cli.ok(["new", ...project, other, "say two"]);

    // The heading names the first project; its name picks another.
    const heading = window.getByRole("heading", { level: 1 });
    await expect(heading).toHaveText(/repo/);
    await window.getByRole("button", { name: t("composer:pick_project") }).click();
    await window.getByRole("menuitem", { name: "other" }).click();
    await expect(heading).toHaveText(/other/);
    await window.getByRole("button", { name: t("composer:pick_agent") }).click();
    await window.getByRole("menuitem", { name: t("sidebar:agent_claude") }).click();
    await expect(window.getByText(t("composer:workspace_local"), { exact: true })).toBeVisible();

    const composer = window.locator("[data-composer] textarea");
    await composer.fill("say three");
    await composer.press("Enter");
    // The session page of the new session, with the agent's answer.
    await expect(window.getByRole("main").getByText("three", { exact: true })).toBeVisible();
    // As the index has it: written a moment later.
    const sessions = async () => (await cupper.cli.run(["sessions"])).lines;
    const { project_id } = (await sessions()).find((line) => line.session_id === second) ?? {};
    await expect
      .poll(async () => (await sessions()).find((line) => line.title === "say three"))
      .toMatchObject({ project_id, agent: "claude", workspace_kind: "local" });
  },
);
