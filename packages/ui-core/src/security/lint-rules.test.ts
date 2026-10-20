// UNIT-ui-lint-rules: a sample that breaks each rule, and one that keeps it,
// linted with the repository's own configuration as files of the main
// document's code would be.
import { ESLint } from "eslint";
import { describe, expect, test } from "vitest";

/** The repository, with its trailing slash. */
const root = decodeURIComponent(new URL("../../../../", import.meta.url).pathname);
const eslint = new ESLint({ cwd: root });

const UI = "packages/ui-core/src/sample.tsx";
const PRELOAD = "apps/desktop/src/preload/sample.ts";
const POLICIES = "packages/ui-core/src/security/policies.ts";

/** The rules `code` breaks, as a file at `file`. */
async function broken(code: string, file = UI): Promise<string[]> {
  const [result] = await eslint.lintText(code, { filePath: `${root}${file}` });
  return (result?.messages ?? []).map((m) => m.ruleId ?? m.message);
}

/** Each sample breaks `rule`, and only it. */
async function each(rule: string, samples: string[], file = UI) {
  for (const sample of samples) {
    expect(await broken(sample, file), sample).toEqual([rule]);
  }
}

const SYNTAX = "no-restricted-syntax";

describe("UNIT-ui-lint-rules", () => {
  test("markup, eval and windows: in ui-core and in the preload alike", async () => {
    const markup = [
      "export const f = (el: Element, x: string) => { el.innerHTML = x; };",
      "export const f = (el: Element) => el.outerHTML;",
      'export const f = (el: Element, x: string) => { el["innerHTML"] = x; };',
      "export const f = (el: Element, x: string) => el.insertAdjacentHTML('beforeend', x);",
      "export const C = (p: { x: string }) => <div dangerouslySetInnerHTML={{ __html: p.x }} />;",
    ];
    await each(SYNTAX, markup);
    await each(SYNTAX, markup.slice(0, 4), PRELOAD);
    await each("no-restricted-properties", [
      "document.write('x');",
      "document.writeln('x');",
      "window.open('https://example.com');",
    ]);
    await each("no-restricted-globals", ["open('https://example.com');"]);
    await each("no-eval", ["eval('1');"], PRELOAD);
    await each("no-new-func", ["export const f = new Function('return 1');"]);
    expect(
      await broken("export const f = (el: Element, x: string) => { el.textContent = x; };"),
    ).toEqual([]);
  });

  test("ui-core imports neither Electron nor Node; the preload may", async () => {
    await each("no-restricted-imports", [
      'import { ipcRenderer } from "electron";\nexport { ipcRenderer };',
      'import { readFile } from "node:fs";\nexport { readFile };',
      'import { join } from "path";\nexport { join };',
      'import { webUtils } from "electron/renderer";\nexport { webUtils };',
    ]);
    expect(
      await broken('import { ipcRenderer } from "electron";\nexport { ipcRenderer };', PRELOAD),
    ).toEqual([]);
  });

  test("Trusted Types policies: policies.ts alone, cupper-worker alone", async () => {
    await each(SYNTAX, ["trustedTypes.createPolicy('x', {});"]);
    await each(
      SYNTAX,
      ["trustedTypes.createPolicy('default', {});", "trustedTypes.createPolicy(name, {});"],
      POLICIES,
    );
    expect(
      await broken("export const p = (f: F) => f.createPolicy(WORKER_POLICY, {});", POLICIES),
    ).toEqual([]);
  });

  test("no style text, rule or custom property", async () => {
    await each(SYNTAX, [
      "export const s = document.createElement('style');",
      "export const C = (p: { css: string }) => <style>{p.css}</style>;",
      "export const f = (el: HTMLElement, v: string) => el.style.setProperty('--w', v);",
      "export const f = (el: HTMLElement, k: string, v: string) => el.style.setProperty(k, v);",
      "export const C = (p: { v: string }) => <div style={{ '--w': p.v }} />;",
      "export const C = (p: { k: string }) => <div style={{ [p.k]: 1 }} />;",
      "export const f = (el: HTMLElement, v: string) => { el.style.cssText = v; };",
      "export const f = (el: HTMLElement, v: string) => el.setAttribute('style', v);",
    ]);
    await each("no-restricted-properties", [
      "export const f = (s: CSSStyleSheet, r: string) => s.insertRule(r);",
      "export const f = (s: CSSStyleSheet, r: string) => s.replaceSync(r);",
      "document.adoptedStyleSheets = [];",
    ]);
    expect(
      await broken(
        "export const f = (el: HTMLElement, v: string) => el.style.setProperty('width', v);",
      ),
    ).toEqual([]);
    expect(
      await broken("export const C = (p: { w: number }) => <div style={{ width: p.w }} />;"),
    ).toEqual([]);
  });

  test("UI text comes from i18n; punctuation, digits and names pass", async () => {
    await each(SYNTAX, [
      "export const C = () => <p>Hello</p>;",
      "export const C = () => <p>你好</p>;",
      'export const C = () => <p>{"Hello"}</p>;',
      "export const C = (p: { n: number }) => <p>{`${p.n} items`}</p>;",
      'export const C = () => <button aria-label="Close" />;',
      'export const C = () => <img alt={"logo"} />;',
      "export const C = () => <b title={`x`} />;",
    ]);
    for (const sample of [
      "export const C = (p: { t: (k: string) => string }) => <p>{p.t('shell:close')}</p>;",
      "export const C = () => <p>· 3/4 —</p>;",
      "export const C = () => <p>Cupper</p>;",
      "export const C = (p: { t: (k: string) => string }) => <button aria-label={p.t('shell:close')} />;",
      'export const C = () => <svg width="16" viewBox="0 0 16 16" />;',
    ]) {
      expect(await broken(sample), sample).toEqual([]);
    }
  });
});
