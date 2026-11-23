// Session management on the real app (design 5.7.4, 5.5.9; UI-0; T265, T267).
// E2E-archive-cleanup archives two worktree sessions, one from its row's menu
// and one by ⌘⇧A: the clean one with its worktree, the one with an untracked
// file keeping it, as the dialog says why. E2E-workspace-open hands the
// workspace to the terminal, the editor and Finder through the e2e build's
// stand-in for `open`, `<data dir>/opener`, which logs its arguments.

import { randomUUID } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test, type Cupper } from "./fixture";
import { texts, untilEvent, untilSession } from "./helpers";
import { focusWindow, shortcut } from "./input";
import { asWindow } from "./security";

/** A session in a new worktree of the test's repository, once its turn is over: the worktree. */
async function worktreeSession(cupper: Cupper, project_id: unknown, text: string) {
  const workspace = { kind: "worktree", base_ref: "main" };
  const message = [{ type: "text", text }];
  const params = { command_id: randomUUID(), agent: "claude", project_id, workspace, message };
  const { result, error } = await asWindow(cupper, "session/create", params);
  expect(error).toBeUndefined();
  const stream = `session/${String((result as { session_id: string }).session_id)}`;
  const ready = await untilEvent(cupper, stream, (event) => event.type === "workspace.ready");
  await untilEvent(cupper, stream, (event) => event.type === "turn.completed");
  return String(ready.data.path);
}

test(
  "E2E-archive-cleanup: a clean worktree goes with its session; one with work in it stays, as the dialog says",
  { tag: "@focus" },
  async ({ cupper }) => {
    await cupper.launch();
    await cupper.connected();
    const t = await texts(cupper);
    const project = ["--agent", "claude", "--project", cupper.repo];
    const { session_id } = await cupper.cli.ok(["new", ...project, "say one"]);
    const { project_id } = await untilSession(cupper, String(session_id));
    const clean = await worktreeSession(cupper, project_id, "say clean");
    const dirty = await worktreeSession(cupper, project_id, "say dirty");
    writeFileSync(join(dirty, "notes.txt"), "not committed\n");
    const window = cupper.window;
    const sidebar = window.getByRole("navigation", { name: t("shell:sidebar") });
    const row = (title: string) => sidebar.getByRole("button", { name: new RegExp(title) });
    const dialog = window.getByRole("dialog");

    // From the row's right-click menu: nothing would be lost, so the worktree may go too.
    await row("say clean").click({ button: "right" });
    await window.getByRole("menuitem", { name: t("session:archive") }).click();
    await expect(dialog).toContainText(t("session:archive_remove_ask"));
    await dialog.getByRole("button", { name: t("session:archive_remove") }).click();
    const removed = t("session:archived_removed", { title: "say clean" });
    await expect(window.getByText(removed)).toBeVisible();
    await expect(row("say clean")).toHaveCount(0);
    await expect.poll(() => existsSync(clean)).toBe(false);

    // ⌘⇧A on the session shown: its untracked file keeps the worktree.
    await row("say dirty").click();
    await focusWindow(cupper);
    await shortcut(cupper, "Command+Shift+A");
    await expect(dialog).toContainText(t("session:blocker_untracked_files"));
    await dialog.getByRole("button", { name: t("session:archive"), exact: true }).click();
    await expect(
      window.getByText(t("session:archived_kept", { title: "say dirty" })),
    ).toBeVisible();
    await expect(row("say dirty")).toHaveCount(0);
    await expect(window.getByRole("heading", { level: 1 })).toHaveText(/repo/);
    expect(readFileSync(join(dirty, "notes.txt"), "utf8")).toBe("not committed\n");
  },
);

test("E2E-workspace-open: 打开 hands the workspace to the terminal, the editor and Finder", async ({
  cupper,
}) => {
  await cupper.launch();
  await cupper.connected();
  const t = await texts(cupper);
  const log = join(cupper.dataDir, "opened.log");
  const opener = `#!/bin/sh\nprintf '%s|' "$@" >> '${log}'\necho >> '${log}'\n`;
  writeFileSync(join(cupper.dataDir, "opener"), opener, { mode: 0o755 });
  await cupper.cli.ok(["new", "--agent", "claude", "--project", cupper.repo, "say hi"]);
  const window = cupper.window;
  await window.getByRole("button", { name: /say hi/ }).click();

  const opened = () => (existsSync(log) ? readFileSync(log, "utf8").split("\n") : []);
  const expected: string[] = [];
  for (const [target, app] of [
    ["terminal", "Terminal"],
    ["editor", "TextEdit"],
    ["finder", "Finder"],
  ] as const) {
    await window.getByRole("button", { name: t("shell:open"), exact: true }).click();
    await window.getByRole("menuitem", { name: t(`shell:open_${target}`) }).click();
    expected.push(`-a|${app}|${cupper.repo}|`);
    await expect.poll(opened).toEqual([...expected, ""]);
  }
});
