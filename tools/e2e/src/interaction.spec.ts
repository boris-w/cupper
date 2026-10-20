// The request panel on the real app (design 5.7.4, 5.7.8; T083, T165): each
// key and click goes through the main process's input guard as real input,
// and what the host recorded as the answer is what the tests check. Keys and
// clicks need the window's focus: every test here is `@focus`.

import { expect, test, type Cupper } from "./fixture";
import { events, texts, untilEvent } from "./helpers";
import { activatePanel, focusWindow, panelReady, press, requestPanel, type } from "./input";

/** Starts a Claude session on `script` from the CLI, opens it, and focuses the window. */
async function session(cupper: Cupper, script: string) {
  await cupper.launch();
  await cupper.connected();
  const project = ["--agent", "claude", "--project", cupper.repo];
  const { session_id } = await cupper.cli.ok(["new", ...project, script]);
  await cupper.window.getByRole("button", { name: script.slice(0, 12) }).click();
  await focusWindow(cupper);
  return { id: String(session_id), stream: `session/${String(session_id)}` };
}

/** The `n`-th request of `stream` (from 0), once the panel shows it and takes input. */
async function request(cupper: Cupper, stream: string, n: number): Promise<string> {
  const t = await texts(cupper);
  const asked = () => events(cupper, stream).filter((e) => e.type === "interaction.requested");
  await expect.poll(() => asked().length, { message: `request ${n}` }).toBeGreaterThan(n);
  const id = String(asked()[n]?.data.request_id);
  // The panel shows the first request still pending; the earlier ones were
  // answered before the agent asked this one.
  const record = cupper.window.locator(`[data-request-record="${id}"]`);
  await expect(record).toHaveText(t("request:record_pending"));
  await panelReady(cupper);
  return id;
}

/** What the host recorded as the answer to request `id`. */
const outcome = async (cupper: Cupper, stream: string, id: string) =>
  (
    await untilEvent(
      cupper,
      stream,
      (e) => e.type === "interaction.resolved" && e.data.request_id === id,
    )
  ).data.outcome;

const selected = (option_id: string) => ({ type: "selected", option_id });

test(
  "E2E-approve-enter-esc: keys and clicks on the request panel answer as mapped",
  { tag: ["@focus", "@smoke"] },
  async ({ cupper }) => {
    // Claude's options: 1 allow once, 2 always allow (privileged), 3 reject.
    const steps = 8;
    const script = Array.from({ length: steps }, (_, n) => `permission k${n}`).join("; ");
    const { stream } = await session(cupper, script);
    const t = await texts(cupper);
    const panel = requestPanel(cupper);
    const keys = (...names: string[]) =>
      async function () {
        await activatePanel(cupper);
        for (const name of names) await press(cupper, name);
      };
    const button = (name: string) => () => panel.getByRole("button", { name }).click();
    const answers: [() => Promise<unknown>, string][] = [
      // Enter takes the option selected at first (RP-4).
      [keys("Enter"), "allow-once"],
      // ↑ ↓ move the selection, the privileged option included; Enter takes it.
      [keys("Down", "Down", "Up", "Down", "Enter"), "reject"],
      // A digit picks its option by number and submits it.
      [keys("3"), "reject"],
      // Esc, "跳过" and "关闭": the rejection that is not persistent.
      [keys("Escape"), "reject"],
      [button(t("request:skip")), "reject"],
      [button(t("request:close")), "reject"],
      // A click on an option answers with it; "提交" with the one selected.
      [() => panel.getByRole("option").first().click(), "allow-once"],
      [button(t("request:submit")), "allow-once"],
    ];
    for (const [n, [answer, option]] of answers.entries()) {
      const id = await request(cupper, stream, n);
      await answer();
      expect(await outcome(cupper, stream, id), `request ${n}`).toEqual(selected(option));
    }
    // None of them took the native dialog of a privileged option.
    expect(await cupper.dialogs.calls()).toEqual([]);
  },
);

/**
 * A required multiple choice. The fake agent's forms have single choices
 * only, so it writes this `elicitation/create` itself; the host answers an id
 * the agent does not wait for, and the host's record is what counts.
 */
const SIZES = JSON.stringify({
  jsonrpc: "2.0",
  id: "e2e-sizes",
  method: "elicitation/create",
  params: {
    mode: "form",
    sessionId: "${session}",
    message: "Sizes?",
    requestedSchema: {
      type: "object",
      required: ["sizes"],
      properties: {
        sizes: {
          type: "array",
          title: "Sizes",
          items: { anyOf: ["s", "m", "l"].map((size) => ({ const: size, title: size })) },
        },
      },
    },
  },
});

test(
  "E2E-question-panel: single and multiple choices and text reach the host as answered",
  { tag: ["@focus", "@smoke"] },
  async ({ cupper }) => {
    const script = `form Color=red|green|blue Size=s|m; question Name?; raw ${SIZES}; wait_cancel`;
    const { stream } = await session(cupper, script);
    const t = await texts(cupper);
    const panel = requestPanel(cupper);
    const answer = (values: object) => ({ type: "answer", values });

    // Two single choices: ← → go between them, a digit answers one and moves
    // on, "跳过" leaves the last one empty and sends the answers.
    const form = await request(cupper, stream, 0);
    await activatePanel(cupper);
    await press(cupper, "Right");
    await expect(panel.getByText("Size", { exact: true })).toBeVisible();
    await press(cupper, "Left");
    await press(cupper, "2");
    await expect(panel.getByText("Size", { exact: true })).toBeVisible();
    await panel.getByRole("button", { name: t("request:skip") }).click();
    expect(await outcome(cupper, stream, form)).toEqual(answer({ question_0: "green" }));
    await expect(cupper.window.getByText('form:{"question_0":"green"}')).toBeVisible();

    // Text: typed as keys, then "提交" clicked.
    const name = await request(cupper, stream, 1);
    await panel.getByRole("textbox", { name: t("request:custom") }).click();
    await type(cupper, "Ada");
    await panel.getByRole("button", { name: t("request:submit") }).click();
    expect(await outcome(cupper, stream, name)).toEqual(answer({ answer: "Ada" }));
    await expect(cupper.window.getByText("answer:Ada")).toBeVisible();

    // Multiple choices: digits tick, Enter sends the ticked ones.
    const sizes = await request(cupper, stream, 2);
    await activatePanel(cupper);
    await press(cupper, "1");
    await press(cupper, "3");
    const ticked = panel.getByRole("option", { checked: true });
    await expect(ticked).toHaveText([/s/, /l/]);
    await press(cupper, "Enter");
    expect(await outcome(cupper, stream, sizes)).toEqual(answer({ sizes: ["s", "l"] }));
  },
);

test(
  "E2E-persistent-option-native: an always-allow option is the native dialog's to answer",
  { tag: "@focus" },
  async ({ cupper }) => {
    const { stream } = await session(cupper, "permission p0; say done");
    const t = await texts(cupper);
    const id = await request(cupper, stream, 0);

    // Its number brings up the native dialog, refusing by default.
    await activatePanel(cupper);
    await press(cupper, "2");
    const asked = await cupper.dialogs.next(0);
    expect(asked).toMatchObject({
      kind: "message",
      message: t("privileged:interaction_title"),
      buttons: [t("privileged:allow"), t("privileged:deny")],
      defaultId: 1,
      cancelId: 1,
    });
    expect(asked.detail).toContain("Yes, and always allow fake commands");
    expect(asked.detail).toContain(t("privileged:effect_persistent"));

    // Cancelled, the request still waits; after the cooldown a cancel starts
    // (2 s), a click on the option asks again, and allowed, the host has it.
    await cupper.dialogs.answer(asked.index, 1);
    await cupper.window.waitForTimeout(2100);
    expect(events(cupper, stream).map((e) => e.type)).not.toContain("interaction.resolved");
    await requestPanel(cupper).getByRole("option").nth(1).click();
    const again = await cupper.dialogs.next(1);
    await cupper.dialogs.answer(again.index, 0);
    expect(await outcome(cupper, stream, id)).toEqual(selected("allow-with-updates"));
    await expect(cupper.window.getByText("permission:allow-with-updates")).toBeVisible();
  },
);

/** The first event of `stream` with `type` whose data mentions `text`. */
const mentioning = (cupper: Cupper, stream: string, type: string, text: string) =>
  untilEvent(cupper, stream, (e) => e.type === type && JSON.stringify(e.data).includes(text));

test(
  "E2E-reject-with-message: the rejection first, then the message, queued and tied to it",
  { tag: ["@focus", "@smoke"] },
  async ({ cupper }) => {
    // The turn goes on to ask again (Claude's turn outlives a rejection): it
    // still runs when the follow-up comes, whatever the machine's load.
    const { id: sessionId, stream } = await session(
      cupper,
      "permission r0; permission r1; say done",
    );
    const t = await texts(cupper);
    const panel = requestPanel(cupper);
    const id = await request(cupper, stream, 0);

    // The free row is number 4, after the options: its number only focuses it.
    await activatePanel(cupper);
    await press(cupper, "4");
    const row = panel.getByRole("textbox", { name: t("request:message", { agent: "Claude" }) });
    await expect(row).toBeFocused();
    await type(cupper, "nope");
    // Enter right after typing is no real input (rule 3): it only says to click.
    await press(cupper, "Enter");
    await expect(panel.getByText(t("request:click_submit"))).toBeVisible();
    await panel.getByRole("button", { name: t("request:submit") }).click();

    expect(await outcome(cupper, stream, id)).toEqual(selected("reject"));
    const resolved = await mentioning(cupper, stream, "interaction.resolved", id);
    // Queued while the turn still runs (M1a queues), then sent as its own turn.
    const queued = await mentioning(cupper, stream, "queue.updated", `"reply_to":"${id}"`);
    await cupper.cli.ok(["approve", sessionId, await request(cupper, stream, 1)]);
    const sent = await mentioning(cupper, stream, "item.started", `"reply_to":"${id}"`);
    expect(resolved.seq).toBeLessThan(queued.seq);
    expect(sent.data).toMatchObject({ kind: "user_message", content: [{ text: "nope" }] });
    await expect(cupper.window.getByText("fake reply to: nope")).toBeVisible();
  },
);

test(
  "E2E-reject-with-message: answered first from the CLI, the text goes back after the draft",
  { tag: ["@focus", "@smoke"] },
  async ({ cupper }) => {
    const { id: sessionId, stream } = await session(cupper, "say ready");
    const t = await texts(cupper);
    const panel = requestPanel(cupper);
    await expect(cupper.window.getByText("ready", { exact: true })).toBeVisible();
    const composer = cupper.window.locator("[data-composer] textarea");
    await composer.fill("draft");
    await cupper.cli.ok(["send", sessionId, "permission r0; say done"]);
    const id = await request(cupper, stream, 0);
    // With a draft the panel waits for Tab or a click (RP-3).
    await expect(panel).toHaveAttribute("data-activation", "manual");
    await activatePanel(cupper);
    await press(cupper, "4");
    await type(cupper, "nope");

    // The CLI approves before the window has drawn that: its views redraw
    // on animation frames, held back here until the user has submitted.
    await cupper.window.evaluate(() => {
      const held: FrameRequestCallback[] = [];
      const frame = window.requestAnimationFrame;
      window.requestAnimationFrame = (callback) => held.push(callback);
      const release = () => {
        window.requestAnimationFrame = frame;
        held.forEach((callback) => frame(callback));
      };
      Object.assign(window, { __e2eRelease: release });
    });
    await cupper.cli.ok(["approve", sessionId, id]);
    await mentioning(cupper, stream, "interaction.resolved", id);
    await panel.getByRole("button", { name: t("request:submit") }).click();
    await expect(cupper.window.getByText(t("request:elsewhere"))).toBeVisible();
    await cupper.window.evaluate(() =>
      (window as unknown as { __e2eRelease(): void }).__e2eRelease(),
    );

    await expect(composer).toHaveValue("draft\n\nnope");
    expect(await outcome(cupper, stream, id)).toEqual(selected("allow-once"));
    // No follow-up went: no event has the text.
    expect(events(cupper, stream).filter((e) => JSON.stringify(e.data).includes("nope"))).toEqual(
      [],
    );
  },
);
