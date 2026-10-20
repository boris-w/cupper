// The session view as the stream's rows (design 5.7.6): each turn's header,
// then its entries; the tail after the last turn; the events of types this
// build does not know at the end. A tool call starts an activity group with
// the tool calls right after it that share its `group_id`; a group of one is
// a plain tool row.

import type { Placeholder, SessionView } from "@cupper/host-client";
import type { Entry, ItemView } from "@cupper/protocol";

import type { StreamRow, ToolData } from "./stream";

type ViewJson = ReturnType<SessionView["toJSON"]>;

/** Where the history before the window stands. */
export type History = "none" | "more" | "loading" | "failed";

export function flatten(view: ViewJson, unknown: Placeholder[], history: History): StreamRow[] {
  const rows: StreamRow[] =
    history === "none" ? [] : [{ kind: "history", key: "history", state: history }];
  for (const turn of view.turns) {
    const { turn_id, turn_no, started_at } = turn.started;
    const completed = turn.completed ?? undefined;
    rows.push({
      kind: "turn_header",
      key: `turn:${turn_id}`,
      turnId: turn_id,
      turnNo: turn_no,
      status: completed?.status ?? "running",
      startedAt: started_at,
      ...(completed ? { durationMs: completed.duration_ms } : {}),
    });
    entries(view, turn.entries, rows);
  }
  entries(view, view.tail, rows);
  unknown.forEach(({ body }, n) =>
    rows.push({ kind: "unknown", key: `unknown:${n}`, type: body.type, body }),
  );
  return rows;
}

function entries(view: ViewJson, list: Entry[], rows: StreamRow[]): void {
  let group: { id: string; tools: ToolData[] } | undefined;
  const close = () => {
    const [first, ...rest] = group?.tools ?? [];
    if (group && first) {
      const running = group.tools.some((tool) => tool.running);
      rows.push(
        rest.length === 0
          ? { kind: "tool", key: `item:${first.itemId}`, ...first }
          : {
              kind: "activity_group",
              key: `group:${first.itemId}`,
              groupId: group.id,
              tools: group.tools,
              running,
            },
      );
    }
    group = undefined;
  };
  for (const entry of list) {
    if ("request" in entry) {
      close();
      rows.push({ kind: "request", key: `request:${entry.request}`, requestId: entry.request });
      continue;
    }
    const item = view.items[entry.item];
    if (item?.item.kind === "tool_call") {
      const tool = toolData(item);
      const id = item.item.group_id ?? undefined;
      if (group === undefined || id === undefined || id !== group.id) {
        close();
      }
      if (id === undefined) {
        rows.push({ kind: "tool", key: `item:${tool.itemId}`, ...tool });
      } else {
        group ??= { id, tools: [] };
        group.tools.push(tool);
      }
      continue;
    }
    close();
    const row = item && itemRow(item);
    if (row) {
      rows.push(row);
    }
  }
  close();
}

type Started = ItemView["item"];
type Known = "user_message" | "tool_call" | "notice" | "error" | "plan" | "late_tool_output";
type Fields<K extends Known> = Omit<
  Extract<Started, { kind: K }>,
  "kind" | "item_id" | "group_id" | "started_at" | "turn_id"
>;

const COMMON = new Set(["kind", "item_id", "group_id", "started_at", "turn_id"]);

/** The fields of an item of kind `K` besides those every item has. */
function fields<K extends Known>(item: Started): Fields<K> {
  const rest = Object.entries(item).filter(([key]) => !COMMON.has(key));
  return Object.fromEntries(rest) as Fields<K>;
}

function toolData({ item, output, end }: ItemView): ToolData {
  const call = fields<"tool_call">(item);
  return { itemId: item.item_id, call, output, running: !end, ...(end ? { end } : {}) };
}

function itemRow({ item, text, end }: ItemView): StreamRow | undefined {
  const key = `item:${item.item_id}`;
  const itemId = item.item_id;
  switch (item.kind) {
    case "user_message": {
      const { content, steered, reply_to } = fields<"user_message">(item);
      return {
        kind: "user_message",
        key,
        itemId,
        content,
        steered,
        ...(reply_to ? { replyTo: reply_to } : {}),
      };
    }
    case "agent_message":
      return { kind: "agent_message", key, itemId, text, streaming: !end };
    case "thought": {
      const duration = end ? { durationMs: end.duration_ms } : {};
      return { kind: "thought", key, itemId, text, running: !end, ...duration };
    }
    case "notice":
      return { kind: "notice", key, itemId, notice: fields<"notice">(item) };
    case "error":
      return { kind: "error", key, itemId, error: fields<"error">(item) };
    case "plan":
      return { kind: "plan", key, itemId, plan: fields<"plan">(item) };
    case "late_tool_output":
      return {
        kind: "late_tool_output",
        key,
        itemId,
        output: fields<"late_tool_output">(item),
        text,
      };
    default:
      return { kind: "unknown", key, type: String(item.kind) };
  }
}
