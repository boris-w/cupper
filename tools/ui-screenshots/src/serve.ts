// Loads the screenshot page the way the shell serves the main document: from
// its own origin, under `MAIN_DOCUMENT_CSP`, with a fixed clock.

import { readFileSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { extname, resolve, sep } from "node:path";

import type { MethodFixture } from "@cupper/host-client/mock";
import { MAIN_DOCUMENT_CSP } from "@cupper/shell-api";
import type { Page } from "@playwright/test";

import type { PageFixture } from "./fixture";

const ROOT = resolve(import.meta.dirname, "../dist/page");
const ORIGIN = "https://cupper.test";
const TYPES: Record<string, string> = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".css": "text/css",
  ".woff2": "font/woff2",
};
/** `protocol/fixtures/methods.json`: the mock host's answers unless a case sets its own. */
const METHODS = JSON.parse(
  readFileSync(resolve(import.meta.dirname, "../../../protocol/fixtures/methods.json"), "utf8"),
) as MethodFixture[];

/** Opens the page with `fixture`; resolves once the host is connected and the fonts are in. */
export async function open(page: Page, fixture: Partial<PageFixture> = {}): Promise<void> {
  await page.clock.setFixedTime(new Date("2026-01-01T09:00:00+08:00"));
  await page.route(`${ORIGIN}/**`, async (route) => {
    const { pathname } = new URL(route.request().url());
    const file = resolve(ROOT, `.${pathname === "/" ? "/index.html" : pathname}`);
    const body = file.startsWith(ROOT + sep) ? await readFile(file).catch(() => null) : null;
    await (body
      ? route.fulfill({
          body,
          contentType: TYPES[extname(file)],
          headers: { "content-security-policy": MAIN_DOCUMENT_CSP },
        })
      : route.fulfill({ status: 404 }));
  });
  await page.addInitScript(
    (value: PageFixture) => {
      window.cupperFixture = value;
      window.cupperViolations = [];
      document.addEventListener("securitypolicyviolation", (event) => {
        const { effectiveDirective, blockedURI, sourceFile, lineNumber, sample } = event;
        window.cupperViolations.push(
          `${effectiveDirective} ${blockedURI} at ${sourceFile}:${lineNumber}: ${sample}`,
        );
      });
    },
    { info: {}, methods: METHODS, streams: {}, ...fixture },
  );
  await page.goto(`${ORIGIN}/`);
  await page.waitForFunction(() => window.cupperHost?.connections.some((conn) => conn.initialized));
  await page.evaluate(() => document.fonts.ready);
}

export const violations = (page: Page) => page.evaluate(() => window.cupperViolations);
