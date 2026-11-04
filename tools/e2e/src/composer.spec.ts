// The composer's file references (design 5.7.4, CP-2): `@` and its list by
// keys, and files dropped on the composer, as the shell gives their paths.

import { writeFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "./fixture";
import { texts, untilEvent } from "./helpers";
import { onScreen } from "./nfr1";

test("E2E-file-ref: @ finds a file by keys; a dropped file in the workspace is referenced", async ({
  cupper,
}) => {
  const stream = await onScreen(cupper, "say ready");
  const t = await texts(cupper);
  const window = cupper.window;
  const composer = window.locator("[data-composer]");
  const area = composer.locator("textarea");

  await area.fill("look at @READ");
  const list = window.getByRole("listbox", { name: t("composer:file_ref") });
  await expect(list.getByRole("option", { name: "README.md" })).toBeVisible();
  await area.press("Enter");
  await expect(list).toHaveCount(0);
  await expect(area).toHaveValue("look at ");
  await expect(composer.getByText("README.md", { exact: true })).toBeVisible();

  // Files with paths of their own, as dropped ones have: one in the
  // workspace, one beside it. No real drag: the drop event carries them.
  writeFileSync(join(cupper.repo, "dropped.txt"), "d\n");
  writeFileSync(join(cupper.root, "outside.txt"), "o\n");
  await window.evaluate(() => {
    const input = Object.assign(document.createElement("input"), { type: "file", multiple: true });
    input.id = "e2e-files";
    document.body.append(input);
  });
  await window.setInputFiles("#e2e-files", [
    join(cupper.repo, "dropped.txt"),
    join(cupper.root, "outside.txt"),
  ]);
  await window.evaluate(() => {
    const input = document.querySelector<HTMLInputElement>("#e2e-files");
    const dataTransfer = new DataTransfer();
    [...(input?.files ?? [])].forEach((file) => dataTransfer.items.add(file));
    const drop = new DragEvent("drop", { dataTransfer, bubbles: true, cancelable: true });
    document.querySelector("[data-composer]")?.dispatchEvent(drop);
    input?.remove();
  });
  await expect(composer.getByText("dropped.txt", { exact: true })).toBeVisible();
  await expect(window.getByText(t("composer:file_outside"))).toBeVisible();
  await expect(composer.getByText("outside.txt")).toHaveCount(0);

  // Both go with the next message, relative to the workspace.
  await area.press("Enter");
  const sent = await untilEvent(cupper, stream, (event) =>
    JSON.stringify(event.data).includes('"type":"file_ref"'),
  );
  const text = JSON.stringify(sent.data);
  expect(text).toContain('"path":"README.md"');
  expect(text).toContain('"path":"dropped.txt"');
});
