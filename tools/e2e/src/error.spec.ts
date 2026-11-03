// The error item on the real app (MS-5; T226 writes it, T143 shows it): a
// turn that fails on the fake agent shows a warning box, with the agent's
// words and the guidance's button or hint.

import { expect, test } from "./fixture";
import { texts, untilEvent, type StoredEvent } from "./helpers";

test("E2E-error-guidance: a failed turn shows its error and what to do", async ({ cupper }) => {
  await cupper.launch();
  await cupper.connected();
  const t = await texts(cupper);
  const project = ["--agent", "claude", "--project", cupper.repo];
  /** The session of `script`, open once its turn wrote the `error` item of `code`. */
  const failed = async (script: string, code: string) => {
    const { session_id } = await cupper.cli.ok(["new", ...project, script]);
    const error = (e: StoredEvent) =>
      e.type === "item.started" && e.data.kind === "error" && e.data.code === code;
    await untilEvent(cupper, `session/${String(session_id)}`, error);
    await cupper.window.getByRole("button", { name: script.slice(0, 12) }).click();
    return cupper.window.getByRole("alert").filter({ hasText: t(`stream:error_${code}`) });
  };

  // Logged out: the command to log in with, to copy; no retry.
  const login = await failed("auth_none", "not_logged_in");
  await expect(login).toContainText(t("stream:guidance_run_login_in_terminal"));
  await expect(login.locator("code")).toHaveText("claude");
  await expect(login.getByRole("button")).toHaveText([t("stream:copy_command")]);

  // The adapter exits under the turn: the end of its stderr, and a retry.
  // The pause lets the host read the line before the exit.
  const exited = await failed("stderr e2e-boom; sleep 100; exit 3", "adapter_exited");
  await expect(exited).toContainText("e2e-boom");
  await expect(exited.getByRole("button")).toHaveText([t("stream:retry")]);
});
