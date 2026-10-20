// Q26 probe, the runner (design 13 Q26, T077). Usage:
//   node run.mjs <directory with node_modules/playwright-core> <Electron executable>
// It launches main.cjs with Playwright's Electron support and prints one
// JSON object with the five findings.
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const [playwrightDir, executablePath] = process.argv.slice(2);
const { _electron: electron } = createRequire(join(playwrightDir, "x.js"))("playwright-core");
const here = dirname(fileURLToPath(import.meta.url));

const app = await electron.launch({ executablePath, args: [join(here, "main.cjs")] });
const page = await app.firstWindow();
await page.waitForSelector("#main");
const logs = () => app.evaluate(() => globalThis.q26.log);
const clear = () => app.evaluate(() => globalThis.q26.clear());
const send = (event) => app.evaluate((_electron, e) => globalThis.q26.send(e), event);
const focusedFrame = () => app.evaluate(() => globalThis.q26.focusedFrame());
const pageKeys = async () => page.evaluate(() => window.keys.splice(0));
const settle = () => page.waitForTimeout(300);

const result = {};

// 1. Playwright's keyboard and before-input-event.
await page.focus("#main");
await clear();
await pageKeys();
await page.keyboard.press("Enter");
await settle();
result.playwrightKeyboard = { ...(await logs()), pageKeys: await pageKeys() };

// 2. sendInputEvent: before-input-event, and isTrusted in the page.
await clear();
await send({ type: "keyDown", keyCode: "Return" });
await send({ type: "keyUp", keyCode: "Return" });
await settle();
result.sendInputEvent = { ...(await logs()), pageKeys: await pageKeys() };

// 3. Keyboard input-events carry key codes at run time? See the input-events
//    of 1 and 2: their fields as Electron gives them.
result.keyboardInputEventFields = [
  ...result.playwrightKeyboard.inputEvent,
  ...result.sendInputEvent.inputEvent,
]
  .filter((e) => /key|char/i.test(e.type))
  .map((e) => Object.keys(e).sort().join(","));

// 4. page.click() and input-event.
await clear();
await page.click("#b");
await settle();
result.pageClick = (await logs()).inputEvent.map((e) => ({ type: e.type, button: e.button }));

// 5. focusedFrame: the main document's field, then the iframe's.
await page.focus("#main");
const inMain = await focusedFrame();
await page.frameLocator("#f").locator("#inner").focus();
const inFrame = await focusedFrame();
result.focusedFrame = { inMain, inFrame };

console.log(JSON.stringify(result, null, 2));
await app.close();
