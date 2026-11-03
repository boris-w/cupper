// @vitest-environment jsdom
import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

import { I18nProvider, ShellProvider } from "../app/providers";
import { highlight, language } from "./highlight";
import { Markdown } from "./markdown";

afterEach(cleanup);

const API = { window: { onFocusChange: () => () => undefined } } as never;

const show = (text: string, streaming = false) =>
  render(
    <ShellProvider api={API} info={{ locale: "zh-CN" } as never}>
      <I18nProvider locale="zh-CN">
        <Markdown text={text} streaming={streaming} />
      </I18nProvider>
    </ShellProvider>,
  );

const pre = (container: HTMLElement) => container.querySelector("pre") as HTMLElement;

describe("UI-code-highlight", () => {
  test("tokens fall into the seven categories; the rest keeps the text's colour", async () => {
    const lines = await highlight('const n = 42; // why\nfn("x");', "typescript");
    const pieces = lines.flat().filter((piece) => piece.text.trim() !== "");
    const of = (text: string) => pieces.find((piece) => piece.text === text)?.category;
    expect([of("const"), of("n"), of("42"), of("// why"), of('"x"'), of("fn")]).toEqual([
      "keyword",
      "variable",
      "literal",
      "comment",
      "string",
      "name",
    ]);
    expect(of("=")).toBeUndefined();
    expect(lines.map((line) => line.map((piece) => piece.text).join(""))).toEqual([
      "const n = 42; // why",
      'fn("x");',
    ]);
  });

  test("a closed fence is highlighted with the app's classes only, its text unchanged", async () => {
    const { container } = show("```ts\nlet a = 1;\n```");
    await waitFor(() => expect(pre(container).dataset.highlighted).toBe(""));
    const classes = [...container.querySelectorAll("pre [class]")].map((e) => e.className);
    expect(classes.length).toBeGreaterThan(0);
    expect(classes.every((name) => /^hl-[a-z]+$/.test(name))).toBe(true);
    expect(container.querySelectorAll("[style], [class*=language-]").length).toBe(0);
    expect(pre(container).textContent).toBe("let a = 1;\n");
  });

  test("an open fence stays plain while the message streams, and is highlighted once closed", async () => {
    const { container, rerender } = show("```ts\nlet a = 1;", true);
    await new Promise((done) => setTimeout(done, 300));
    expect(pre(container).dataset.highlighted).toBeUndefined();
    rerender(
      <ShellProvider api={API} info={{ locale: "zh-CN" } as never}>
        <I18nProvider locale="zh-CN">
          <Markdown text={"```ts\nlet a = 1;\n```\n\nmore"} streaming />
        </I18nProvider>
      </ShellProvider>,
    );
    await waitFor(() => expect(pre(container).dataset.highlighted).toBe(""));
  });

  test("a language not in the list, or none, stays plain", () => {
    expect([language("brainfuck"), language(undefined), language("constructor")]).toEqual([
      undefined,
      undefined,
      undefined,
    ]);
    expect([language("sh"), language("rust")]).toEqual(["shellscript", "rust"]);
    const { container } = show("```brainfuck\n+++\n```");
    expect(pre(container).querySelectorAll("span").length).toBe(0);
  });
});
