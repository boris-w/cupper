// The turn's change card on the real app (design 5.7.4, 5.7.8; T132): the
// agent changes a file, the card shows it, and "撤销" reverts the turn only
// after the confirmation, whose click is real input through the main
// process's guard. The click needs the window's focus: `@focus`.

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "./fixture";
import { texts } from "./helpers";
import { click, focusWindow } from "./input";

test(
  "E2E-turn-changes-undo: the card's undo asks first, then the turn's change is gone",
  { tag: "@focus" },
  async ({ cupper }) => {
    await cupper.launch();
    await cupper.connected();
    const t = await texts(cupper);
    const readme = () => readFileSync(join(cupper.repo, "README.md"), "utf8");
    const script = "write_file README.md changed\\n; say 改好了";
    await cupper.cli.ok(["new", "--agent", "claude", "--project", cupper.repo, script]);
    await cupper.window.getByRole("button", { name: script.slice(0, 12) }).click();

    const card = cupper.window.locator("[data-turn-changes]");
    await expect(card).toContainText(t("stream:changes_title", { count: 1 }));
    await expect(card.getByText("README.md")).toBeVisible();
    expect(readme()).toBe("changed\n");

    await focusWindow(cupper);
    await card.getByRole("button", { name: t("stream:changes_undo") }).click();
    const confirm = card.getByRole("alertdialog");
    await expect(confirm).toHaveAttribute("data-ready", "true");
    // Nothing reverts before the confirmation.
    expect(readme()).toBe("changed\n");
    // From the card's own button, the focus is on "确认" (RP-11); the click is real input.
    const yes = confirm.getByRole("button", { name: t("review:confirm") });
    await expect(yes).toBeFocused();
    await click(cupper, yes);
    await expect.poll(readme, { message: "the turn's change is reverted" }).toBe("e2e\n");
    await expect(confirm).toBeHidden();
  },
);
