// T104's main process: `apps/desktop`'s `app` protocol, CSP, window,
// permissions and inputGuard as they are, on two origins; the only frame
// navigation let through is the review document's first load (the M1b
// exception to `navigation.ts`). One run: scenario 9 cold (the 5,000-line
// diff), the checks of Q29, the JSON to $T104_RESULTS; then it quits.
import { execFileSync } from "node:child_process";
import { mkdirSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

import { MAIN_DOCUMENT_CSP } from "@cupper/shell-api";
import { app, BrowserWindow, clipboard, Menu, nativeTheme, protocol, session } from "electron";
import type { WebContents, WebFrameMain } from "electron";

import { watchInput } from "../../../apps/desktop/src/main/input";
import {
  APP_SCHEME,
  APP_SCHEME_PRIVILEGES,
  appProtocolHandler,
} from "../../../apps/desktop/src/main/security/app-protocol";
import { restrictPermissions } from "../../../apps/desktop/src/main/security/permissions";
import { mainWindowOptions } from "../../../apps/desktop/src/main/window";
import { REVIEW_DOCUMENT_CSP } from "./csp";

const REVIEW_URL = "app://cupper-review/index.html";
const out = process.env.T104_RESULTS ?? "results/local.json";
const flag = (name: string) => process.env[name] === "1";
if (process.env.T104_USER_DATA) app.setPath("userData", process.env.T104_USER_DATA);
protocol.registerSchemesAsPrivileged([APP_SCHEME_PRIVILEGES]);

const sleep = (ms: number) => new Promise((done) => setTimeout(done, ms));
async function until<T>(read: () => Promise<T>, what: string, ms = 60_000): Promise<T> {
  for (const end = Date.now() + ms; Date.now() < end; await sleep(50)) {
    const value = await read().catch(() => undefined);
    if (value) return value;
  }
  throw new Error(`timed out: ${what}`);
}
const js = (frame: WebFrameMain, code: string, gesture = false) =>
  frame.executeJavaScript(code, gesture);
const press = (contents: WebContents, keyCode: string, modifiers: ("meta" | "shift")[] = []) => {
  for (const type of ["keyDown", "char", "keyUp"] as const)
    contents.sendInputEvent({ type, keyCode, modifiers });
};
/** Through DevTools, as Playwright types: it reaches the focused frame, and no `before-input-event`. */
const typeIn = async (contents: WebContents, key: string, modifiers = 0) => {
  if (!contents.debugger.isAttached()) contents.debugger.attach();
  for (const type of ["keyDown", "keyUp"]) {
    const text = type === "keyDown" && key.length === 1 ? key : undefined;
    await contents.debugger.sendCommand("Input.dispatchKeyEvent", { type, key, modifiers, text });
  }
};
const focusIn = (frame: WebFrameMain) => js(frame, "document.body.tabIndex = -1, document.body.focus(), 1");

async function run() {
  const dist = __dirname;
  const hosts = {
    cupper: { root: join(dist, "cupper"), csp: MAIN_DOCUMENT_CSP },
    "cupper-review": { root: join(dist, "review"), csp: REVIEW_DOCUMENT_CSP },
  };
  protocol.handle(APP_SCHEME, appProtocolHandler(hosts));
  restrictPermissions(session.defaultSession);
  const accelerated: string[] = [];
  const probe = { label: "Probe", accelerator: "CmdOrCtrl+Shift+P", click: () => accelerated.push("menu") };
  Menu.setApplicationMenu(Menu.buildFromTemplate([{ label: "T104", submenu: [probe] }]));
  const window = new BrowserWindow(mainWindowOptions("e2e", join(dist, "preload.cjs")));
  const contents = window.webContents;
  const guard = watchInput(window, Date.now);
  const blocked: string[] = [];
  let reviewLoaded = false;
  contents.on("will-navigate", (event) => (event.preventDefault(), blocked.push(event.url)));
  contents.on("will-frame-navigate", (event) => {
    if (!event.isMainFrame && !reviewLoaded && event.url === REVIEW_URL) return void (reviewLoaded = true);
    event.preventDefault();
    blocked.push(event.url);
  });
  contents.setWindowOpenHandler(() => ({ action: "deny" }));
  let cspConsole = 0;
  contents.on("console-message", (event) => void (/Content Security Policy/.test(event.message) && cspConsole++));
  window.once("ready-to-show", () => window.show());

  await window.loadURL("app://cupper/index.html");
  const main = contents.mainFrame;
  await until(() => js(main, "t104.seen.ready > 0"), "ready");
  const review = main.frames[0];
  await until(() => js(review, "t104.seen.highlighted > 0"), "highlighted");
  const [mainSeen, reviewSeen] = [await js(main, "t104.seen"), await js(review, "t104.seen")];
  // At the top, where the line with markup is (security (i)).
  const injectedAsText = await js(review, "t104.shadow('[data-line]').some((el) => el.textContent.includes('<img src=x')) && t104.shadow('img').length === 0");
  const metrics = app.getAppMetrics();
  const memoryOf = (pid: number) => metrics.find((m) => m.pid === pid)?.memory;
  const scrollFrames: number[] = await js(review, "t104.scrollFrames()");
  if (flag("T104_SHOTS")) writeFileSync(join(dirname(out), "review-light.png"), (await window.capturePage()).toPNG());

  // Point 4 (before 2, 3): the theme goes over the port only; the system's appearance reaches the iframe.
  const lightAfterStray = await js(review, "document.documentElement.dataset.theme === 'light'");
  await js(main, "t104.port.postMessage({ type: 'theme_changed', theme: 'dark' })");
  const themed = await until(() => js(review, "document.documentElement.dataset.theme === 'dark'"), "theme").catch(() => false);
  nativeTheme.themeSource = "dark";
  const system = await until(() => js(review, "matchMedia('(prefers-color-scheme: dark)').matches"), "system").catch(() => false);
  if (flag("T104_SHOTS")) writeFileSync(join(dirname(out), "review-dark.png"), (await window.capturePage()).toPNG());
  nativeTheme.themeSource = "system";

  // Points 2, 3: keys with the focus in the review document, then in the main frame.
  await js(main, "document.querySelector('iframe').focus(), 1");
  await focusIn(review);
  const reviewFocused = contents.focusedFrame?.frameTreeNodeId === review.frameTreeNodeId;
  const seenByMain: string[] = [];
  contents.on("before-input-event", (_event, input) => {
    const frame = contents.focusedFrame?.frameTreeNodeId === review.frameTreeNodeId ? "review" : "main";
    if (input.type === "keyDown") seenByMain.push(`${input.key}@${frame}`);
  });
  contents.focus();
  await sleep(1100);
  for (const key of ["A", "Escape", "Return"]) press(contents, key);
  press(contents, "P", ["meta", "shift"]);
  for (const key of ["a", "Escape", "Enter"]) await typeIn(contents, key);
  await typeIn(contents, "p", 4 | 8);
  await sleep(300);
  const forwarded: string[] = await js(main, "t104.seen.forwarded");
  const reviewKeys: string[] = await js(review, "t104.seen.keys");
  const evidenceFromReview = guard.take() ?? null;
  const menuFromReview = accelerated.length;
  let keystroke: string | null = null;
  if (flag("T104_KEYSTROKE")) {
    app.focus({ steal: true });
    await focusIn(review);
    const script = 'tell application "System Events" to keystroke "p" using {command down, shift down}';
    try {
      execFileSync("osascript", ["-e", script]);
      await sleep(500);
      keystroke = `menu ${accelerated.length - menuFromReview}`;
    } catch (error) {
      keystroke = String(error).slice(0, 200);
    }
  }
  await focusIn(main);
  press(contents, "Return");
  await sleep(200);
  const evidenceFromMain = guard.take() ?? null;

  // Point 6, runner only: the review document writes the clipboard.
  let clipboardWrite: unknown = null;
  if (flag("T104_CLIPBOARD")) {
    await focusIn(review);
    const wrote = await js(review, "navigator.clipboard.writeText('t104').then(() => 'ok', (e) => e.name)", true);
    clipboardWrite = { wrote, read: clipboard.readText() === "t104" };
  }

  // Point 7: scroll_to from the main document.
  await js(main, "t104.port.postMessage({ type: 'scroll_to', line: 4000 })");
  const inView = "(() => { const r = t104.line(4000)?.getBoundingClientRect(); return !!r && r.top >= 0 && r.bottom <= innerHeight })()";
  const scrolledTo = await until(() => js(review, inView), "scroll_to", 5000).catch(() => false);

  // Isolation, navigation, constructable stylesheets, injection.
  await js(review, "location.href = 'app://cupper-review/elsewhere.html', 1").catch(() => undefined);
  await sleep(300);
  const checks = {
    origins: [main.origin, review.origin],
    outOfProcess: review.osProcessId !== main.osProcessId,
    parentDocument: await js(review, "(() => { try { return parent.document && 'readable' } catch (e) { return e.name } })()"),
    preload: { main: await js(main, "window.t104Preload === true"), review: await js(review, "'t104Preload' in window") },
    ports: { accepted: reviewSeen.accepted, ignored: reviewSeen.ignored, strays: mainSeen.strays },
    workers: reviewSeen.workers,
    rowsInDom: await js(review, "t104.shadow('[data-line]').length"),
    adoptedStyleSheets: await js(review, "[...document.querySelectorAll('*')].reduce((n, el) => n + (el.shadowRoot?.adoptedStyleSheets.length ?? 0), document.adoptedStyleSheets.length)"),
    styleElements: await js(review, "t104.shadow('style').length + document.querySelectorAll('style').length"),
    injectedAsText,
    wasmInBundle: readdirSync(join(dist, "review")).filter((file) => file.endsWith(".wasm")),
    reviewUrlAfterNavigation: main.frames[0]?.url,
    blocked,
    lightAfterStray,
    themed,
    system,
    lang: await js(review, "document.documentElement.lang"),
    reviewFocused,
    forwarded,
    reviewKeys,
    seenByMain,
    evidenceFromReview,
    evidenceFromMain,
    menuFromReview,
    keystroke,
    clipboardWrite,
    scrolledTo,
  };
  const timings = {
    readyMs: mainSeen.ready - mainSeen.created,
    rowsMs: reviewSeen.rows - mainSeen.created,
    highlightedMs: reviewSeen.highlighted - mainSeen.created,
    sinceLaunchMs: reviewSeen.highlighted - performance.timeOrigin,
  };
  const violations = {
    main: await js(main, "t104.seen.violations"),
    review: await js(review, "t104.seen.violations"),
    console: cspConsole,
  };
  const memory = { mainRenderer: memoryOf(main.osProcessId), reviewRenderer: memoryOf(review.osProcessId), all: metrics.map((m) => ({ type: m.type, memory: m.memory })) };
  return { timings, memory, scrollFrames, violations, checks };
}

app.whenReady().then(async () => {
  mkdirSync(dirname(out), { recursive: true });
  let code = 0;
  try {
    writeFileSync(out, JSON.stringify(await run(), null, 1));
  } catch (error) {
    writeFileSync(out, JSON.stringify({ error: String(error) }));
    code = 1;
  }
  app.exit(code);
});
