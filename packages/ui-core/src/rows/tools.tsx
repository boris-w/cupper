// Tool activity in the stream (design 5.7.4 MS-3): one line per call, an icon
// and a past-tense summary from the semantic tool kind (`@cupper/i18n`), a
// shimmer while it runs, and a command's output block on demand. Calls in a
// row with one `group_id` fold into one summary line. Agent text (paths,
// commands, titles, output) is text only, its invisible characters escaped.

import type { ItemEnd, ToolCall } from "@cupper/protocol";
import { escapeInvisible } from "@cupper/i18n";
import { useState } from "react";

import { useT, type Translate } from "../app/providers";
import { ChevronDownIcon, ToolIcon } from "../components/icons";
import { AnsiText } from "./ansi";

/** A tool call as its row shows it (T082's `ToolData`). */
export interface Tool {
  itemId: string;
  call: ToolCall;
  /** The output streamed so far. */
  output: string;
  running: boolean;
  end?: ItemEnd;
}

const KINDS = ["read", "edit", "delete", "move", "search", "execute", "fetch", "switch_mode"];
const kindOf = (call: ToolCall) => (KINDS.includes(call.tool_kind) ? call.tool_kind : "other");

/** What the call acted on: its command, query, URL, paths, else the agent's title. */
function target(t: Translate, call: ToolCall): string {
  const [first, ...rest] = call.paths;
  const named =
    (call.tool_kind === "execute" && call.command) ||
    (call.tool_kind === "search" && call.query) ||
    (call.tool_kind === "fetch" && call.url) ||
    (first !== undefined &&
      (rest.length ? t("stream:paths_more", { first, count: call.paths.length }) : first)) ||
    call.title;
  return escapeInvisible(named);
}

/** "已读取 a.rs", or "正在读取 a.rs" while it runs. */
export function toolText(t: Translate, call: ToolCall, running: boolean): string {
  const key = `stream:tool_${kindOf(call)}${running ? "_running" : ""}`;
  return t(key, { target: target(t, call) });
}

/** "运行了 2 条命令，读取了 3 个文件": the counts by kind, in order of appearance. */
export function groupText(t: Translate, calls: ToolCall[]): string {
  const counts = new Map<string, number>();
  for (const call of calls) {
    const kind = ["read", "edit", "search", "execute"].includes(call.tool_kind)
      ? call.tool_kind
      : "other";
    counts.set(kind, (counts.get(kind) ?? 0) + 1);
  }
  const parts = [...counts].map(([kind, count]) => t(`stream:group_${kind}`, { count }));
  return parts.join(t("stream:group_separator"));
}

/** The output so far, else what the call carries inline. */
function outputOf({ output, call }: Tool): string {
  const inline = call.output?.type === "text" ? (call.output as { text: string }).text : "";
  return output || inline || (call.output_tail ?? "");
}

export function ToolRow({ tool }: { tool: Tool }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const output = outputOf(tool);
  const expandable = tool.call.tool_kind === "execute" && output !== "";
  return (
    <div className="text-chat-sm">
      <button
        type="button"
        disabled={!expandable}
        aria-expanded={expandable ? open : undefined}
        onClick={() => setOpen(!open)}
        className="flex w-full min-w-0 items-center gap-2 text-left text-fg-tertiary"
      >
        <span className="shrink-0 text-icon-tertiary">
          <ToolIcon kind={kindOf(tool.call)} />
        </span>
        <span className={`truncate ${tool.running ? "shimmer" : ""}`}>
          {toolText(t, tool.call, tool.running)}
        </span>
        {tool.end?.status === "failed" && (
          <span className="shrink-0 text-danger">{t("stream:tool_failed")}</span>
        )}
      </button>
      {open && <OutputBlock tool={tool} output={output} />}
    </div>
  );
}

/** A command's output (design 5.7.4): a 13 title bar, then the monospace body. */
function OutputBlock({ tool, output }: { tool: Tool; output: string }) {
  const t = useT();
  const { command, exit_code: code, output_state: state } = tool.call;
  return (
    <div className="mt-1 overflow-hidden rounded-lg border border-border bg-surface-code">
      <div className="flex items-center gap-2 border-b border-border px-3 py-1.5 text-fg-secondary">
        <span className="min-w-0 flex-1 truncate font-mono">{escapeInvisible(command ?? "")}</span>
        {code != null && <span className="shrink-0">{t("stream:exit_code", { code })}</span>}
      </div>
      <pre className="max-h-60 overflow-auto px-3 py-2 font-mono text-code whitespace-pre-wrap">
        {state === "truncated" && <div className="text-fg-tertiary">{t("stream:output_more")}</div>}
        <AnsiText text={output} />
      </pre>
    </div>
  );
}

/** Calls of one activity group: one summary line, the list on demand (at most 224, faded). */
export function ActivityGroup({ tools, running }: { tools: Tool[]; running: boolean }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  return (
    <div className="group text-chat-sm">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="flex min-w-0 items-center gap-1 text-left text-fg-tertiary"
      >
        <span className={`truncate ${running ? "shimmer" : ""}`}>
          {groupText(
            t,
            tools.map((tool) => tool.call),
          )}
        </span>
        <span className={`shrink-0 opacity-0 group-hover:opacity-100 ${open ? "" : "-rotate-90"}`}>
          <ChevronDownIcon />
        </span>
      </button>
      {open && (
        <div className="fade-y mt-1 max-h-56 space-y-1 overflow-y-auto py-1">
          {tools.map((tool) => (
            <ToolRow key={tool.itemId} tool={tool} />
          ))}
        </div>
      )}
    </div>
  );
}
