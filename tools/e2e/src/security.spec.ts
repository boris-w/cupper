// Security in the window (design 5.9.4, 5.7.8): the main document's CSP and
// Trusted Types, real input, the request panel's arming and numbering, the
// privileged methods, and a second `initialize`.

import { expect, test } from "./fixture";
import { texts, untilEvent } from "./helpers";
import { panelReady, press, requestPanel } from "./input";
import { onScreen, openSession, session } from "./nfr1";
import { permission, refocus, resolved } from "./security";

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
  { tag: "@focus" },
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
    // Activated again by focusing the options (a click would be real input).
    await requestPanel(cupper)
      .getByRole("listbox")
      .evaluate((list: HTMLElement) => {
        list.blur();
        list.focus();
      });
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
