// @vitest-environment jsdom
import { HostClient } from "@cupper/host-client";
import { MockHost } from "@cupper/host-client/mock";
import type { ToolCall } from "@cupper/protocol";
import type { Handshake } from "@cupper/shell-api";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { HostContext, I18nProvider, ShellProvider } from "../app/providers";
import { ansiSpans, AnsiText } from "./ansi";
import "./index";
import { Markdown } from "./markdown";
import { RowView } from "./registry";
import { streamRows, type StreamRow, type ToolData } from "./stream";

afterEach(cleanup);

function show(node: ReactNode) {
  const openExternal = vi.fn();
  const view = render(
    <ShellProvider
      api={{ openExternal, window: { onFocusChange: () => () => undefined } } as never}
      info={{ locale: "zh-CN" } as never}
    >
      <I18nProvider locale="zh-CN">{node}</I18nProvider>
    </ShellProvider>,
  );
  return { ...view, openExternal };
}

const ESC = String.fromCharCode(27);

/** No element of `container` came from markup, scripts or styles of an agent. */
function inert(container: HTMLElement) {
  expect(container.querySelectorAll("script, style, img, iframe, object, embed").length).toBe(0);
  for (const element of container.querySelectorAll("*")) {
    const names = element.getAttributeNames();
    expect(names.filter((name) => name.startsWith("on") || name === "style")).toEqual([]);
    const href = element.getAttribute("href");
    expect(href === null || /^https?:\/\//.test(href)).toBe(true);
    for (const name of element.classList) {
      // Only the app's classes: streamdown's, ours, the ANSI list.
      expect(name).not.toMatch(/^evil$|^language-/);
    }
  }
  expect((window as unknown as Record<string, unknown>).hacked).toBeUndefined();
}

const call = (fields: Partial<ToolCall>): ToolCall => ({
  extraction: "full",
  output_state: "complete",
  paths: [],
  title: "",
  tool_kind: "other",
  ...fields,
});

describe("UNIT-md-injection", () => {
  test("Markdown renders no markup, script, picture or style of the agent", async () => {
    const md = [
      "# Title",
      "<script>window.hacked = 1</script>",
      '<img src="x" onerror="window.hacked = 2">',
      "[click](javascript:window.hacked=3) [site](https://example.com) [mail](mailto:a@b.c)",
      "![remote](https://example.com/a.png) ![data](data:image/png;base64,AAAA)",
      '<style>body{display:none}</style><div style="color:red" class="evil">styled</div>',
      "```css\nbody { color: red }\n```",
    ].join("\n\n");
    const { container, openExternal } = show(<Markdown text={md} streaming={false} />);
    inert(container);
    // Raw HTML stays text; a link that is not http(s) is its text only.
    expect(container.textContent).toContain("<script>window.hacked = 1</script>");
    expect(screen.getByText("click").tagName).toBe("SPAN");
    expect(screen.getByText("mail").tagName).toBe("SPAN");
    // A picture is a link to it, a data picture its text.
    expect(screen.getByText("remote").getAttribute("href")).toBe("https://example.com/a.png");
    expect(screen.getByText("data").tagName).toBe("SPAN");
    // A link opens outside the app, never in the document.
    await userEvent.click(screen.getByText("site"));
    expect(openExternal).toHaveBeenCalledWith("https://example.com/");
  });

  test("ANSI colours become classes of a fixed list; other escapes are dropped", () => {
    const output = `${ESC}[31mred${ESC}[0m plain ${ESC}[1;38;2;255;0;0mbold${ESC}]0;title\u0007${ESC}[2K<style>x{}</style>`;
    expect(ansiSpans(output)).toEqual([
      { text: "red", classes: "ansi-red" },
      { text: " plain ", classes: "" },
      { text: "bold<style>x{}</style>", classes: "ansi-bold" },
    ]);
    const { container } = show(<AnsiText text={`${ESC}[4;92;2mok`} />);
    inert(container);
    expect(container.querySelector("span")?.className).toBe("ansi-green ansi-dim ansi-underline");
  });

  test("tool, thought and error rows show agent text as text, invisible characters escaped", async () => {
    const tool = (fields: Partial<ToolCall>, output = ""): ToolData => ({
      itemId: "i_1",
      call: call(fields),
      output,
      running: false,
    });
    const command = "echo hi\u202E<img src=x onerror=1>";
    const run = tool({ tool_kind: "execute", command, exit_code: 1 }, `${ESC}[31mfail`);
    const rows: StreamRow[] = [
      { kind: "tool", key: "1", ...run },
      { kind: "tool", key: "2", ...tool({ tool_kind: "read", paths: ["a.rs"] }), running: true },
      { kind: "tool", key: "3", ...tool({ tool_kind: "edit", paths: ["a.rs", "b.rs"] }) },
      { kind: "tool", key: "4", ...tool({ tool_kind: "mcp", title: "lookup" }) },
      {
        ...{ kind: "activity_group", key: "5", groupId: "g_1", running: false },
        tools: [run, tool({ tool_kind: "execute" }), tool({ tool_kind: "read" })],
      },
      {
        kind: "thought",
        key: "6",
        itemId: "i_2",
        text: "plan\u200B",
        running: false,
        durationMs: 65_000,
      },
      {
        ...{ kind: "error", key: "7", itemId: "i_3" },
        error: { code: "quota_exhausted", guidance: "none", retryable: false, raw: "<b>x</b>" },
      },
    ];
    const { container } = show(
      rows.map((row) => <RowView key={row.key} registry={streamRows} row={row} />),
    );
    const lines = [...container.querySelectorAll("button")].map((b) => b.textContent);
    expect(lines).toEqual([
      "已运行 echo hi\\u{202E}<img src=x onerror=1>",
      "正在读取 a.rs",
      "已编辑 a.rs 等 2 个文件",
      "lookup",
      "运行了 2 条命令，读取了 1 个文件",
      "已思考 1 分 5 秒",
    ]);
    await userEvent.click(screen.getAllByRole("button")[0] as HTMLElement);
    await userEvent.click(screen.getByText("已思考 1 分 5 秒"));
    expect(screen.getByText("退出码 1")).toBeTruthy();
    expect(screen.getByText("plan\\u{200B}")).toBeTruthy();
    expect(screen.getByText("<b>x</b>")).toBeTruthy();
    inert(container);
  });
});

describe("UNIT-blob-fulltext: a long text in the user's bubble", () => {
  test("its size until 显示更多, which downloads it; then the text, folding as ever", async () => {
    const text = "第一行\n".repeat(3000);
    const bytes = new TextEncoder().encode(text);
    const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
    const hash = Array.from(digest, (b) => b.toString(16).padStart(2, "0")).join("");
    const host = new MockHost([{ method: "initialize", params: {}, result: {} }]);
    host.handle("blob/get", ({ conn }) => {
      conn.sendBlob({ hash, offset: 0, last: true, bytes });
      return { result: { hash, chunks: 1, offset: 0, length: bytes.length, size: bytes.length } };
    });
    const client = new HostClient();
    const conn = host.connect();
    conn.initialized = true;
    client.attach(conn.port(), {} as Handshake);
    const row: StreamRow = {
      ...{ kind: "user_message", key: "1", itemId: "i_1", steered: false },
      content: [
        { type: "text_blob", blob: { hash, media_type: "text/plain", size: bytes.length } },
      ],
    };
    const { container } = show(
      <HostContext value={{ client, state: client.state, handshake: undefined }}>
        <RowView registry={streamRows} row={row} />
      </HostContext>,
    );
    expect(screen.getByText("长文本（30 KB）")).toBeTruthy();
    expect(host.requests.map((request) => request.method)).not.toContain("blob/get");
    await userEvent.click(screen.getByRole("button", { name: "显示更多" }));
    await waitFor(() => expect(container.textContent).toContain("第一行\n第一行"));
    await userEvent.click(screen.getByRole("button", { name: "收起" }));
    expect(screen.getByRole("button", { name: "显示更多" })).toBeTruthy();
  });
});
