import { describe, expect, test } from "vitest";

import { createI18n, DEFAULT_LOCALE, NAMESPACES } from "./i18n";

describe("createI18n", () => {
  test("is ready on return, with every feature file as a namespace", () => {
    const i18n = createI18n();
    expect(i18n.isInitialized).toBe(true);
    expect(NAMESPACES).toEqual(["command", "host", "privileged", "shell", "sidebar", "stream", "update"]);
    expect(i18n.t("command:new_session")).toBe("新建会话");
    expect(i18n.t("update:ready")).toBe("重启以更新");
  });

  test("falls back to the default locale", () => {
    expect(createI18n("en-US").t("command:archive")).toBe("归档会话");
  });

  test("interpolates values as plain text", () => {
    const i18n = createI18n();
    expect(i18n.t("command:session_n", { n: 3 })).toBe("第 3 个会话");
    expect(i18n.t("command:session_n", { n: "<b>&" })).toBe("第 <b>& 个会话");
  });

  test("keys are semantic codes and every text is a non-empty string", () => {
    const i18n = createI18n();
    for (const ns of NAMESPACES) {
      const bundle: Record<string, unknown> = i18n.getResourceBundle(DEFAULT_LOCALE, ns);
      for (const [key, text] of Object.entries(bundle)) {
        expect(key, `${ns}:${key}`).toMatch(/^[a-z][a-z0-9_]*$/);
        expect(text, `${ns}:${key}`).toEqual(expect.stringMatching(/\S/));
      }
    }
  });
});
