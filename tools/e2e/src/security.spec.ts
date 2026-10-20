// Security in the window (design 5.9.4, 5.7.8): the main document's CSP and
// Trusted Types, real input, the request panel's arming and numbering, the
// privileged methods, and a second `initialize`.

import { expect, test } from "./fixture";
import { BUILT } from "./paths";
import { texts, untilEvent } from "./helpers";
import { focusWindow, panelReady, press, pressDigit, requestPanel, type } from "./input";
import { id, onScreen, openSession, session } from "./nfr1";
import { asWindow, digest, focusOptions, permission, refocus, resolved } from "./security";

test(
  "E2E-SEC-csp: no violation on the pages; injected script does not run; no default policy",
  { tag: "@smoke" },
  async ({ cupper }) => {
    const violations: string[] = [];
    await cupper.launch();
    cupper.window.on("console", (message) => {
      const text = message.text();
      if (text.includes("Content Security Policy") || text.includes("Trusted Type")) {
        violations.push(text);
      }
    });
    // From the document's first script on, as the page reports them too.
    await cupper.window.addInitScript(() => {
      const seen: string[] = [];
      Object.assign(window, { __violations: seen });
      document.addEventListener("securitypolicyviolation", (event) =>
        seen.push(`${event.effectiveDirective} ${event.blockedURI}`),
      );
    });
    await cupper.window.reload();
    const reported = () =>
      cupper.window.evaluate(() => (window as unknown as { __violations: string[] }).__violations);
    await cupper.connected();
    const stream = await session(cupper, "say hello; think a thought; tool t1 read README.md");
    await untilEvent(cupper, stream, (event) => event.type === "turn.completed");
    await openSession(cupper, "say hello");
    await permission(cupper, stream);
    await panelReady(cupper);
    const t = await texts(cupper);
    await cupper.window
      .getByRole("button", { name: t("sidebar:project_menu") })
      .first()
      .click();
    await expect(cupper.window.getByRole("menu")).toBeVisible();
    await cupper.window.keyboard.press("Escape");
    expect(violations).toEqual([]);
    expect(await reported()).toEqual([]);

    const injected = await cupper.window.evaluate(() => {
      const script = document.createElement("script");
      try {
        script.textContent = "window.__injected = true";
        document.body.append(script);
      } catch {
        // Trusted Types refuse the text itself.
      }
      return (window as { __injected?: boolean }).__injected === true;
    });
    expect(injected).toBe(false);
    const policy = await cupper.window.evaluate(() => {
      const { trustedTypes } = window as unknown as {
        trustedTypes: { createPolicy(name: string, rules: object): unknown };
      };
      try {
        trustedTypes.createPolicy("default", { createHTML: (text: string) => text });
        return "created";
      } catch (error) {
        return String(error);
      }
    });
    expect(policy).not.toBe("created");
    // What was refused just now is reported: the counts above would have seen it.
    await expect.poll(async () => (await reported()).length).toBeGreaterThan(0);
  },
);

test(
  "E2E-SEC-real-input: Playwright's keys never count, and keys count only after the window's cooldown",
  { tag: ["@focus", "@smoke"] },
  async ({ cupper }) => {
    const stream = await onScreen(cupper, "say first");
    const request = await permission(cupper, stream);
    await panelReady(cupper);
    const t = await texts(cupper);
    // The click that opened the session is real input for a second: let it pass.
    await cupper.window.waitForTimeout(1100);
    // Playwright's own key reaches the page, but it is no real input: refused.
    await requestPanel(cupper).getByRole("listbox").focus();
    await expect(requestPanel(cupper)).toHaveAttribute("data-activation", "auto");
    await cupper.window.keyboard.press("1");
    await expect(requestPanel(cupper).getByText(t("request:manual"))).toBeVisible();
    expect(resolved(cupper, stream, request)).toBe(false);
    // Just after the window gains the focus, Enter is refused and asked again.
    await focusOptions(cupper);
    await expect(requestPanel(cupper)).toHaveAttribute("data-activation", "auto");
    await refocus(cupper);
    await press(cupper, "Enter");
    await expect(requestPanel(cupper).getByText(t("request:press_again"))).toBeVisible();
    expect(resolved(cupper, stream, request)).toBe(false);
    // A second later, the same Enter goes through.
    await cupper.window.waitForTimeout(1100);
    await press(cupper, "Enter");
    await expect(requestPanel(cupper)).toHaveCount(0);
    await untilEvent(cupper, stream, (e) => e.type === "interaction.resolved");
  },
);

test(
  "E2E-SEC-panel-arming: typing or a draft keeps the focus off the panel; a typing key revokes; the first cmd-Enter waits",
  { tag: ["@focus", "@smoke"] },
  async ({ cupper }) => {
    const stream = await onScreen(cupper, "say first");
    const [panel, t] = [requestPanel(cupper), await texts(cupper)];
    const composer = cupper.window.locator("[data-composer] textarea");
    await composer.focus();
    await focusWindow(cupper);
    // A draft: the panel does not take the focus, and a digit does nothing.
    await type(cupper, "hi");
    let request = await permission(cupper, stream);
    await panelReady(cupper);
    await expect(panel).toHaveAttribute("data-activation", "manual");
    await pressDigit(cupper, 1);
    await cupper.window.waitForTimeout(600);
    expect(resolved(cupper, stream, request)).toBe(false);
    // Once active, the first cmd-Enter only says how to submit; the second does.
    await focusOptions(cupper);
    await press(cupper, "Enter", ["meta"]);
    await expect(panel.getByText(t("request:cmd_enter"))).toBeVisible();
    expect(resolved(cupper, stream, request)).toBe(false);
    await press(cupper, "Enter", ["meta"]);
    await expect(panel).toHaveCount(0);
    // Typed just now, even if emptied again: no focus either.
    await composer.fill("");
    await composer.focus();
    await type(cupper, "x");
    await press(cupper, "Backspace");
    request = await permission(cupper, stream);
    await panelReady(cupper);
    await expect(panel).toHaveAttribute("data-activation", "manual");
    await cupper.cli.ok(["approve", id(stream), request]);
    await expect(panel).toHaveCount(0);
    // Empty and quiet for 2 s: the panel takes the focus; a typing key revokes it,
    // and the Enter right after does not submit.
    await composer.focus();
    await cupper.window.waitForTimeout(2100);
    request = await permission(cupper, stream);
    await panelReady(cupper);
    await expect(panel).toHaveAttribute("data-activation", "auto");
    await press(cupper, "a");
    await expect(panel).toHaveAttribute("data-activation", "manual");
    await press(cupper, "Enter");
    await expect(panel.getByText(t("request:manual"))).toBeVisible();
    expect(resolved(cupper, stream, request)).toBe(false);
  },
);

test(
  "E2E-SEC-panel-arming: arming swallows keys and clicks, a click begun before counts not, each request arms, a menu takes the keys",
  { tag: ["@focus", "@smoke"] },
  async ({ cupper }) => {
    const stream = await onScreen(cupper, "say first");
    const panel = requestPanel(cupper);
    // Each panel that comes, as it came: armed or not.
    await cupper.window.evaluate(() => {
      const seen: boolean[] = [];
      Object.assign(window, { __panels: seen });
      new MutationObserver((changes) => {
        for (const node of changes.flatMap((change) => [...change.addedNodes])) {
          const panel = "[data-request-panel]";
          const found =
            node instanceof Element && (node.matches(panel) ? node : node.querySelector(panel));
          if (found) seen.push(found.hasAttribute("data-ready"));
        }
      }).observe(document.body, { childList: true, subtree: true });
    });
    const panels = () =>
      cupper.window.evaluate(() => (window as unknown as { __panels: boolean[] }).__panels);
    await focusWindow(cupper);
    // A press begun on the composer and let go on the panel's first option.
    const box = await cupper.window.locator("[data-composer]").boundingBox();
    await cupper.window.mouse.move((box?.x ?? 0) + 20, (box?.y ?? 0) + 20);
    await cupper.window.mouse.down();
    const first = await permission(cupper, stream, "permission p1; permission p2");
    await expect(panel).toBeVisible();
    // Within the arming time, keys and clicks are swallowed.
    await pressDigit(cupper, 1);
    await panel.getByRole("option").first().click();
    await cupper.window.mouse.up();
    await panelReady(cupper);
    await cupper.window.waitForTimeout(600);
    expect(resolved(cupper, stream, first)).toBe(false);
    // A menu over the panel takes the keys.
    const t = await texts(cupper);
    await cupper.window
      .getByRole("button", { name: t("sidebar:project_menu") })
      .first()
      .click();
    await expect(cupper.window.getByRole("menu")).toBeVisible();
    await pressDigit(cupper, 1);
    await cupper.window.waitForTimeout(600);
    expect(resolved(cupper, stream, first)).toBe(false);
    await press(cupper, "Escape");
    await expect(cupper.window.getByRole("menu")).toHaveCount(0);
    // Answered; the next request's panel comes unarmed.
    await focusOptions(cupper);
    await cupper.window.waitForTimeout(1100);
    await pressDigit(cupper, 1);
    await untilEvent(cupper, stream, (e) => e.type === "interaction.resolved");
    await expect.poll(async () => (await panels()).length).toBe(2);
    expect(await panels()).toEqual([false, false]);
  },
);

test(
  "E2E-SEC-panel-numbering: every option has its number; unknown effect takes none; privileged asks natively; a wrong digit is refused",
  { tag: ["@focus", "@smoke"] },
  async ({ cupper }) => {
    const stream = await onScreen(cupper, "say first");
    const options = [
      ["allow-once", "Yes", "allow_once"],
      ["maybe", "Maybe", "allow_maybe"],
      ["allow-with-updates", "Always", "allow_always"],
      ["reject", "No", "reject_once"],
    ].map(([optionId, name, kind]) => ({ optionId, name, kind }));
    const script = `permission_options ${JSON.stringify(options)}`;
    const request = await permission(cupper, stream, script);
    const [panel, t] = [requestPanel(cupper), await texts(cupper)];
    await panelReady(cupper);
    const items = panel.getByRole("option");
    await expect(items).toHaveText([/^1\.Yes/, /^2\.Maybe/, /^3\.Always/, /^4\.No/]);
    await expect(items.nth(1)).toContainText(t("request:unknown_effect"));
    await expect(items.nth(2)).toContainText(t("request:needs_confirm"));
    await focusWindow(cupper);
    await focusOptions(cupper);
    // 2: unknown effect, no number. 3: privileged, the native dialog asks.
    await pressDigit(cupper, 2);
    await pressDigit(cupper, 3);
    const asked = await cupper.dialogs.next();
    expect(asked).toMatchObject({ kind: "message", message: t("privileged:interaction_title") });
    await cupper.dialogs.answer(asked.index, 1);
    expect(await cupper.dialogs.calls()).toHaveLength(1);
    // The window's answer must carry the digit of its option: off the panel,
    // 4 then option 1 is refused; 1 then option 1 goes through.
    await cupper.window.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    const params = {
      session_id: id(stream),
      request_id: request,
      request_digest: await digest(cupper, stream, request),
      outcome: { type: "selected", option_id: "allow-once" },
    };
    await cupper.window.waitForTimeout(1100);
    await pressDigit(cupper, 4);
    const wrong = await asWindow(cupper, "interaction/respond", params);
    expect(wrong).toMatchObject({ error: { data: { reason: "user_activation_required" } } });
    await pressDigit(cupper, 1);
    expect(await asWindow(cupper, "interaction/respond", params)).toHaveProperty("result");
    await expect(panel).toHaveCount(0);
  },
);

test(
  "E2E-SEC-privileged: the window cannot send privileged methods; the native dialog shows what the host verified",
  { tag: ["@focus", "@smoke"] },
  async ({ cupper }) => {
    await cupper.launch();
    await cupper.connected();
    for (const method of ["host/exit", "auth/elevation_respond"]) {
      const refused = await asWindow(cupper, method, { force: false });
      expect(refused, method).toMatchObject({ error: { data: { code: "forbidden" } } });
    }
    // The CLI asks; the dialog the banner opens names the CLI's executable,
    // which only the host knew. Refused there, the CLI is denied.
    const t = await texts(cupper);
    const repl = cupper.cli.repl("cli");
    const elevation = repl.run("elevate --scopes write", 30_000);
    await cupper.window.getByRole("button", { name: t("sidebar:review") }).click();
    const dialog = await cupper.dialogs.next();
    expect(dialog.message).toBe(t("privileged:elevation_title"));
    expect(dialog.detail).toContain(t("privileged:elevation_executable", { path: BUILT.cli }));
    await cupper.dialogs.answer(dialog.index, 1);
    expect((await elevation).code).toBe(10);
  },
);

test(
  "E2E-SEC-double-initialize: a second initialize on the window's connection stops in the main process",
  { tag: "@smoke" },
  async ({ cupper }) => {
    await cupper.launch();
    await cupper.connected();
    const again = await asWindow(cupper, "initialize", {});
    expect(again).toMatchObject({
      error: { data: { code: "forbidden", reason: "already_initialized" } },
    });
  },
);
