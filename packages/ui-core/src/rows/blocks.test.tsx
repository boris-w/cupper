// @vitest-environment jsdom
import { cleanup, render } from "@testing-library/react";
import type { ReactNode } from "react";
import * as streamdown from "streamdown";
import { afterEach, describe, expect, test, vi } from "vitest";

import { I18nProvider, ShellProvider } from "../app/providers";
import { cut, PIECE, streamingBlocks } from "./blocks";
import { Markdown } from "./markdown";

vi.mock("streamdown", async (actual) => {
  const real = await actual<typeof import("streamdown")>();
  return { ...real, parseMarkdownIntoBlocks: vi.fn(real.parseMarkdownIntoBlocks) };
});

afterEach(cleanup);

const wrap = (node: ReactNode) => (
  <ShellProvider
    api={{ openExternal: vi.fn(), window: { onFocusChange: () => () => undefined } } as never}
    info={{ locale: "zh-CN" } as never}
  >
    <I18nProvider locale="zh-CN">{node}</I18nProvider>
  </ShellProvider>
);

const NORMAL = [
  "# 标题",
  "一段文字，**加粗**，`代码`，[链接](https://example.com)。\n第二行。",
  "```ts\nconst a = 1;\n\nconst b = 2;\n```",
  "- 一\n- 二",
  "| a | b |\n|---|---|\n| 1 | 2 |",
  "结尾",
].join("\n\n");

describe("UNIT-stream-rows: a long message, in pieces while it streams (T159)", () => {
  test("a message of normal length is split as streamdown splits it, at every step", () => {
    const split = streamingBlocks();
    for (let n = 1; n <= NORMAL.length; n++) {
      const text = NORMAL.slice(0, n);
      expect(split(text), JSON.stringify(text)).toEqual(streamdown.parseMarkdownIntoBlocks(text));
    }
  });

  test("a long block is cut after a space before a word, into pieces that make it up", () => {
    const words = Array.from({ length: 6000 }, (_, n) => `word${n}`).join(" ");
    const pieces = cut(words);
    expect(pieces.length).toBeGreaterThan(2);
    expect(pieces.map((piece) => piece.text).join("")).toBe(words);
    for (const [n, piece] of pieces.entries()) {
      expect(piece.text.length).toBeLessThanOrEqual(PIECE);
      if (n > 0) expect(piece.text).toMatch(/^word\d/);
    }
    expect(
      cut("a".repeat(2.5 * PIECE)).map((piece) => piece.length),
      "no space",
    ).toEqual([PIECE, PIECE, PIECE / 2]);
  });

  test("a long fenced block is cut at line ends, each piece fenced as the block is", () => {
    const lines = Array.from({ length: 3000 }, (_, n) => `const v${n} = ${n};\n`).join("");
    const code = `\`\`\`ts\n${lines}\`\`\`\n`;
    const pieces = cut(code);
    expect(pieces.length).toBeGreaterThan(1);
    for (const piece of pieces) expect(piece.text).toMatch(/^```ts\n[^]*```\n$/);
    const own = pieces.map(({ text, length }, n) => text.slice(n > 0 ? 6 : 0).slice(0, length));
    expect(own.join("")).toBe(code);
  });

  test("as a long message grows, only what follows its settled pieces is split again", () => {
    const split = streamingBlocks();
    const parse = vi.mocked(streamdown.parseMarkdownIntoBlocks);
    const words = Array.from({ length: 12_000 }, (_, n) => `w${n}`).join(" ");
    for (let n = 1024; n <= words.length; n += 1024) {
      const text = `# 标题\n\n${words.slice(0, n)}`;
      parse.mockClear();
      const blocks = split(text);
      expect(parse.mock.calls[0]?.[0].length).toBeLessThanOrEqual(PIECE + 1024);
      expect(blocks, `at ${n}`).toEqual(streamingBlocks()(text));
    }
  });

  test("while it streams a long block shows in pieces; once done, as it does without them", () => {
    const text = `# 标题\n\n${"a".repeat(3 * PIECE)}`;
    const view = render(wrap(<Markdown text={text} streaming />));
    expect(view.container.querySelectorAll("p")).toHaveLength(3);
    view.rerender(wrap(<Markdown text={text} streaming={false} />));
    expect(view.container.querySelectorAll("p")).toHaveLength(1);
    const whole = render(wrap(<Markdown text={text} streaming={false} />));
    expect(view.container.isEqualNode(whole.container)).toBe(true);
  });
});
