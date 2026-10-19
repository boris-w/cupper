import { describe, expect, test } from "vitest";

import { escapeInvisible } from "./escape";

/** Each code point, between two letters, comes out as its escape. */
function expectEscaped(codePoints: number[]): void {
  for (const code of codePoints) {
    const hex = code.toString(16).toUpperCase();
    expect(escapeInvisible(`a${String.fromCodePoint(code)}b`), `U+${hex}`).toBe(`a\\u{${hex}}b`);
  }
}

describe("UNIT-i18n-escape", () => {
  test("escapes the bidirectional controls", () => {
    expectEscaped([
      0x061c, 0x200e, 0x200f, 0x202a, 0x202b, 0x202c, 0x202d, 0x202e, 0x2066, 0x2067, 0x2068,
      0x2069,
    ]);
    expect(escapeInvisible("rm -rf ./\u202Ehs.gpj")).toBe("rm -rf ./\\u{202E}hs.gpj");
  });

  test("escapes the zero-width characters", () => {
    expectEscaped([0x200b, 0x200c, 0x200d, 0x2060, 0xfeff, 0x180e]);
    expect(escapeInvisible("git push origin\u200B main")).toBe("git push origin\\u{200B} main");
  });

  test("escapes control characters but keeps tabs and line feeds", () => {
    expectEscaped([0x00, 0x07, 0x08, 0x0b, 0x0c, 0x0d, 0x1b, 0x7f, 0x85, 0x9b]);
    expect(escapeInvisible("echo ok\r\n\u001b[2Kgone")).toBe("echo ok\\u{D}\n\\u{1B}[2Kgone");
    expect(escapeInvisible("a\tb\nc")).toBe("a\tb\nc");
  });

  test("escapes other invisible characters, one escape per code point", () => {
    expectEscaped([
      0x00ad, 0x034f, 0x115f, 0x1160, 0x17b4, 0x17b5, 0x180b, 0x2028, 0x2029, 0x3164, 0xffa0,
      0xfe00, 0xfe0f, 0x1d173, 0xe0001, 0xe0041, 0xe007f, 0xe0100, 0xe01ef,
    ]);
    expect(escapeInvisible("ok\u{E0069}\u{E0067}")).toBe("ok\\u{E0069}\\u{E0067}");
    expect(escapeInvisible("a\uD800b\uDC00")).toBe("a\\u{D800}b\\u{DC00}");
  });

  test("keeps ordinary Chinese and English text as it is", () => {
    for (const text of [
      "",
      "允许 Claude 运行此命令？",
      "Allow Claude to run this command?",
      "全角标点：，。！？「」《》……——",
      "中文\u3000全角空格与\u00A0不换行空格",
      "cat ~/项目/src/main.rs | grep -n 'fn main' > /tmp/out.txt",
      "Ünïcödé café naïve – “quotes” © ½ €",
      "עברית ו-العربية",
      "😀👍🎉",
      "already \\u{202E} escaped",
    ]) {
      expect(escapeInvisible(text)).toBe(text);
    }
  });

  test("escaping again changes nothing", () => {
    const once = escapeInvisible("\u202Eabc\u200B\u0000\r\n");
    expect(escapeInvisible(once)).toBe(once);
  });
});
