// The composer's attachments (design 5.7.4, CP-2): `@` and its list by keys,
// files dropped on the composer as the shell gives their paths, and images.

import { writeFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test, type Cupper } from "./fixture";
import { texts, untilEvent } from "./helpers";
import { onScreen } from "./nfr1";

/**
 * Drops files with paths of their own on the composer, as a drag from the
 * Finder brings them: set on a file input, then carried by a drop event. No
 * real drag is needed.
 */
async function drop(cupper: Cupper, paths: string[]) {
  const window = cupper.window;
  await window.evaluate(() => {
    const input = Object.assign(document.createElement("input"), { type: "file", multiple: true });
    input.id = "e2e-files";
    document.body.append(input);
  });
  await window.setInputFiles("#e2e-files", paths);
  await window.evaluate(() => {
    const input = document.querySelector<HTMLInputElement>("#e2e-files");
    const dataTransfer = new DataTransfer();
    [...(input?.files ?? [])].forEach((file) => dataTransfer.items.add(file));
    const event = new DragEvent("drop", { dataTransfer, bubbles: true, cancelable: true });
    document.querySelector("[data-composer]")?.dispatchEvent(event);
    input?.remove();
  });
}

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

  // Dropped: one file in the workspace, one beside it.
  writeFileSync(join(cupper.repo, "dropped.txt"), "d\n");
  writeFileSync(join(cupper.root, "outside.txt"), "o\n");
  await drop(cupper, [join(cupper.repo, "dropped.txt"), join(cupper.root, "outside.txt")]);
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

// A 1 × 1 PNG.
const PNG =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

test("E2E-image-paste: pasted, dropped and picked images go with the next message", async ({
  cupper,
}) => {
  const stream = await onScreen(cupper, "say ready");
  const t = await texts(cupper);
  const composer = cupper.window.locator("[data-composer]");
  const images = composer.getByRole("img", { name: t("composer:attachment_image") });
  // A paste whose clipboard data holds an image: the event, not the system clipboard.
  await cupper.window.evaluate((png) => {
    const data = new DataTransfer();
    const bytes = Uint8Array.from(atob(png), (c) => c.charCodeAt(0));
    data.items.add(new File([bytes], "pasted.png", { type: "image/png" }));
    const paste = new ClipboardEvent("paste", { clipboardData: data, bubbles: true });
    document.querySelector("[data-composer] textarea")?.dispatchEvent(paste);
  }, PNG);
  await expect(images).toHaveCount(1);
  // Dropped beside a text file, which stays a reference; then picked with "+".
  const shot = join(cupper.root, "shot.png");
  writeFileSync(shot, Buffer.from(PNG, "base64"));
  writeFileSync(join(cupper.repo, "notes.txt"), "n\n");
  await drop(cupper, [shot, join(cupper.repo, "notes.txt")]);
  await expect(images).toHaveCount(2);
  await expect(composer.getByText("notes.txt", { exact: true })).toBeVisible();
  await composer.locator("input[type=file]").setInputFiles(shot);
  await expect(images).toHaveCount(3);

  await composer.locator("textarea").press("Enter");
  const sent = await untilEvent(cupper, stream, (event) =>
    JSON.stringify(event.data).includes('"type":"image"'),
  );
  expect(JSON.stringify(sent.data).match(/"type":"image"/g)).toHaveLength(3);
});
