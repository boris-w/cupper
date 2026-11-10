// The composer's attachments (design 5.7.4, CP-2): `@` and its list by keys,
// files dropped on the composer as the shell gives their paths, and images;
// slash commands (CP-3), the send key preference (design 5.7.7), and
// steering the running turn (CP-6) and a queued message (F3).

import { writeFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test, type Cupper } from "./fixture";
import { setPref, texts, untilEvent } from "./helpers";
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

test("E2E-slash-commands: / lists the agent's commands by search; one taken goes as text", async ({
  cupper,
}) => {
  await onScreen(cupper, "say ready");
  const t = await texts(cupper);
  const window = cupper.window;
  const area = window.locator("[data-composer] textarea");
  const menu = window.getByRole("listbox", { name: t("composer:slash_title") });

  await area.fill("/zz");
  await expect(menu.getByText(t("composer:slash_none"))).toBeVisible();
  await area.fill("/comp");
  await expect(menu.getByRole("option")).toHaveText([/^\/compact/]);
  await area.press("Enter");
  await expect(area).toHaveValue("/compact ");
  await area.pressSequentially("keep it short");
  await area.press("Enter");
  // The fake agent answers with the text it got.
  const reply = window.getByText("fake reply to: /compact keep it short");
  await expect(reply).toBeVisible();

  // The new session page lists the commands the host cached for Claude.
  await window.getByRole("button", { name: t("sidebar:new_session"), exact: true }).click();
  await area.fill("/");
  await expect(menu.getByRole("option", { name: /^\/init/ })).toBeVisible();
});

test("E2E-send-key-setting: with ⌘Enter to send, Enter is a newline; ⌘⇧Enter queues", async ({
  cupper,
}) => {
  const stream = await onScreen(cupper, "say ready");
  const window = cupper.window;
  await setPref(cupper, "composer.sendKey", "mod_enter");
  const area = window.locator("[data-composer] textarea");
  await area.fill("say one");
  await area.press("Enter");
  await expect(area).toHaveValue("say one\n");

  // A turn that runs until stopped, sent with ⌘Enter; behind it, ⌘⇧Enter queues.
  await area.fill("wait_cancel");
  await area.press("Meta+Enter");
  await expect(area).toHaveValue("");
  await untilEvent(cupper, stream, (e) => e.type === "turn.started" && e.data.turn_no === 2);
  await area.fill("say later");
  await area.press("Meta+Shift+Enter");
  await expect(window.locator("[data-queue-preview]")).toHaveText("say later");
});

/** The running turn's user message mentioning `text` that went in as a steer. */
const steered = (cupper: Cupper, stream: string, text: string) =>
  untilEvent(
    cupper,
    stream,
    (e) =>
      e.type === "item.started" && e.data.steered === true && JSON.stringify(e.data).includes(text),
  );

/**
 * A session on screen with a second turn running `script`, sent from the
 * composer. Its first words are on screen: the agent has the prompt, so the
 * host takes a steer (a turn still preparing queues one, `not_prompting`).
 */
async function turnRunning(cupper: Cupper, script: string) {
  const stream = await onScreen(cupper, "say ready");
  const area = cupper.window.locator("[data-composer] textarea");
  await area.fill(`say turn two; ${script}`);
  await area.press("Enter");
  await expect(cupper.window.getByText("turn two", { exact: true })).toBeVisible();
  return { stream, area };
}

test("E2E-composer-steer-default: Enter steers the running turn, ⌘Enter queues; the preference swaps them", async ({
  cupper,
}) => {
  // The persona takes a steer while its prompt runs.
  const { stream, area } = await turnRunning(cupper, "wait_cancel");
  const queue = cupper.window.locator("[data-queue-preview]");
  await area.fill("steer one");
  await area.press("Enter");
  await steered(cupper, stream, "steer one");
  await area.fill("queued one");
  await area.press("Meta+Enter");
  await expect(queue).toHaveText(["queued one"]);

  await setPref(cupper, "composer.followUpQueue", true);
  await area.fill("queued two");
  await area.press("Enter");
  await expect(queue).toHaveText(["queued one", "queued two"]);
  await area.fill("steer two");
  await area.press("Meta+Enter");
  await steered(cupper, stream, "steer two");
});

test("E2E-composer-steer-default: a steer the agent fails goes to the queue and says so", async ({
  cupper,
}) => {
  const { area } = await turnRunning(cupper, "steer_error; wait_cancel");
  const t = await texts(cupper);
  await area.fill("refused");
  await area.press("Enter");
  await expect(cupper.window.getByText(t("composer:queued_instead"))).toBeVisible();
  await expect(cupper.window.locator("[data-queue-preview]")).toHaveText("refused");
});

test("E2E-queue-edit: a queued message edited in place and steered in from its row; the menu switches queueing", async ({
  cupper,
}) => {
  const { stream, area } = await turnRunning(cupper, "wait_cancel");
  const [t, window] = [await texts(cupper), cupper.window];
  const row = window.locator("[data-queue-id]");
  const previews = row.locator("[data-queue-preview]");
  const pick = async (item: string) => {
    await row
      .first()
      .getByRole("button", { name: t("composer:queue_menu") })
      .click();
    await window.getByRole("menuitem", { name: t(item) }).click();
  };
  await area.fill("first draft");
  await area.press("Meta+Enter");
  await expect(previews).toHaveText("first draft");

  await pick("composer:queue_edit");
  const box = row.getByRole("textbox", { name: t("composer:queue_edit") });
  await box.fill("rewritten");
  await box.press("Enter");
  await expect(previews).toHaveText("rewritten");
  await row.getByRole("button", { name: t("composer:steer") }).click();
  await steered(cupper, stream, "rewritten");
  await expect(row).toHaveCount(0);

  // Follow-ups queue by default, from the menu: Enter queues now.
  await area.fill("second");
  await area.press("Meta+Enter");
  await pick("composer:queue_on");
  await area.fill("third");
  await area.press("Enter");
  await expect(previews).toHaveText(["second", "third"]);
});
