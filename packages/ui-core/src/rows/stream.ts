// The message stream's rows (design 5.7.6): what `flatten` makes of a session
// view, one row per line of the virtual list, and the registry the row
// components register with. Each row carries the data it shows, as of the
// latest flush of the view, so a row component is a pure function of `row`.
//
// T082 registers `history`, `turn_header`, `user_message` and `unknown`;
// T166 `agent_message`, `thought`, `tool`, `activity_group`, `notice` and
// `error`; T165 `request`. `plan` and `late_tool_output` wait for T146; until
// a kind is registered its rows show the placeholder of `RowRegistry`.

import type {
  ErrorItem,
  EventBody,
  ItemEnd,
  LateToolOutput,
  Notice,
  Plan,
  PromptBlock,
  RequestView,
  ToolCall,
  TurnStatus,
} from "@cupper/protocol";
import type { ComponentType } from "react";

import { RowRegistry } from "./registry";

/** A tool call as its row shows it. */
export interface ToolData {
  itemId: string;
  call: ToolCall;
  /** The output streamed so far (`item.delta` with `field: output`). */
  output: string;
  /** Until `item.completed`. */
  running: boolean;
  end?: ItemEnd;
}

/** Every row has a `key`, unique and stable in the list. */
export type StreamRow =
  /** "Load earlier history" at the top, while the host has older turns. */
  | { kind: "history"; key: string; state: "more" | "loading" | "failed" }
  | {
      kind: "turn_header";
      key: string;
      turnId: string;
      turnNo: number;
      /** `running` until `turn.completed`. */
      status: "running" | TurnStatus;
      startedAt: number;
      durationMs?: number;
    }
  | {
      kind: "user_message";
      key: string;
      itemId: string;
      content: PromptBlock[];
      steered: boolean;
      replyTo?: string;
      /** What the rejected request asked, in the agent's words (T165). */
      replyTitle?: string;
    }
  /** `text` grows while `streaming`. */
  | { kind: "agent_message"; key: string; itemId: string; text: string; streaming: boolean }
  | {
      kind: "thought";
      key: string;
      itemId: string;
      text: string;
      running: boolean;
      durationMs?: number;
    }
  /** A tool call outside an activity group, or alone in one. */
  | ({ kind: "tool"; key: string } & ToolData)
  /** Two or more tool calls in a row with one `group_id`: one row, its own expansion. */
  | { kind: "activity_group"; key: string; groupId: string; tools: ToolData[]; running: boolean }
  | { kind: "notice"; key: string; itemId: string; notice: Notice }
  | { kind: "error"; key: string; itemId: string; error: ErrorItem }
  | { kind: "plan"; key: string; itemId: string; plan: Plan }
  | {
      kind: "late_tool_output";
      key: string;
      itemId: string;
      output: LateToolOutput;
      text: string;
    }
  /** An interaction request's place in the stream (T165). */
  | { kind: "request"; key: string; requestId: string; request?: RequestView }
  /** An item of a kind, or an event of a type, this build does not know. */
  | { kind: "unknown"; key: string; type: string; body?: EventBody };

export type StreamRowKind = StreamRow["kind"];
export type RowOf<K extends StreamRowKind> = Extract<StreamRow, { kind: K }>;

/** The one registry of the stream's rows. */
export const streamRows = new RowRegistry<StreamRow>();

/** Registers `component` for the rows of `kind`; call once per kind, at module load. */
export function registerRow<K extends StreamRowKind>(
  kind: K,
  component: ComponentType<{ row: RowOf<K> }>,
): void {
  streamRows.register(kind, component as ComponentType<{ row: StreamRow }>);
}
