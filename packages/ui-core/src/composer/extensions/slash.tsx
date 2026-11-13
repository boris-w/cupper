// Slash commands (design 5.7.4, CP-3): `/` at the start of the text opens
// the agent's commands in the popover `@` uses, what follows the `/`
// searching their names. A command taken replaces `/{query}` with
// `/{name} `; the message goes out as text, for the agent to read. The
// commands come from the session's capabilities, or on the new session page
// from the host's cache for the agent picked; with none, nothing opens.

import type { AgentsCapabilitiesResult, SlashCommand } from "@cupper/protocol";
import type { SessionView } from "@cupper/host-client";
import { useQuery } from "@tanstack/react-query";
import { use, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useStore } from "zustand";

import { useHost, useT } from "../../app/providers";
import { useUi } from "../../state/provider";
import { useEntry } from "../../state/stream";
import { WithSession } from "../config";
import { OverlayContext, type ComposerExtension } from "../extensions";
import { OPTION, POPOVER, useCaret, usePopoverKeys } from "../popover";
import { NEW_DRAFT } from "../send";

const NONE: readonly SlashCommand[] = [];

/** What follows the `/` that starts the text, while the caret is in that word. */
export const slashQuery = (text: string, caret: number) =>
  /^\/(\S*)$/.exec(text.slice(0, caret))?.[1];

/** The commands whose name holds `query`, those it starts first. */
export function matching(commands: readonly SlashCommand[], query: string) {
  const q = query.toLowerCase();
  const starts = (c: SlashCommand) => (c.name.toLowerCase().startsWith(q) ? 0 : 1);
  return commands
    .filter((c) => c.name.toLowerCase().includes(q))
    .toSorted((a, b) => starts(a) - starts(b));
}

function Menu({ sessionId, commands }: { sessionId?: string; commands: readonly SlashCommand[] }) {
  const t = useT();
  const ui = useUi();
  const { area, container } = use(OverlayContext);
  const key = sessionId ?? NEW_DRAFT;
  const draft = useStore(ui, (state) => state.drafts[key] ?? "");
  const [caret, setCaret] = useCaret();
  const [closed, setClosed] = useState(false);
  const [selected, setSelected] = useState(0);
  const query = slashQuery(draft, caret);
  const open = query !== undefined && !closed && commands.length > 0;
  const shown = open ? matching(commands, query ?? "") : NONE;

  const back = useRef<number | undefined>(undefined);
  const take = (command: SlashCommand) => {
    const text = `/${command.name} `;
    ui.getState().setDraft(key, text + draft.slice(caret));
    back.current = text.length;
  };
  // The caret goes after the command and its space, once the draft shows them.
  useLayoutEffect(() => {
    if (back.current === undefined || !area) return;
    area.setSelectionRange(back.current, back.current);
    setCaret(back.current);
    back.current = undefined;
  });

  useEffect(() => setSelected(0), [query]);
  // A `/` typed again after Esc opens the menu again.
  const typing = query !== undefined;
  useEffect(() => {
    if (!typing) setClosed(false);
  }, [typing]);
  usePopoverKeys(open, {
    ...{ count: shown.length, selected, select: setSelected },
    take: (index) => shown[index] && take(shown[index]),
    close: () => setClosed(true),
  });

  if (!open || !container) return null;
  return createPortal(
    <div role="listbox" aria-label={t("composer:slash_title")} className={`${POPOVER} max-h-80`}>
      <p className="px-2.5 py-1.5 text-xs text-fg-tertiary">{t("composer:slash_title")}</p>
      {shown.length === 0 && (
        <p className="px-2.5 py-1.5 text-fg-tertiary">{t("composer:slash_none")}</p>
      )}
      {shown.map((command, i) => (
        <div
          key={command.name}
          role="option"
          aria-selected={i === selected}
          // The text area keeps the focus.
          onMouseDown={(event) => {
            event.preventDefault();
            take(command);
          }}
          onMouseEnter={() => setSelected(i)}
          className={OPTION}
        >
          <span className="text-fg">/{command.name}</span>
          {command.input_hint && <span className="ml-1.5">{command.input_hint}</span>}
          <span className="ml-3 text-fg-tertiary">{command.description}</span>
        </div>
      ))}
    </div>,
    container,
  );
}

/** A session's commands, as its capabilities change. */
function Live({ view, sessionId }: { view: SessionView; sessionId: string }) {
  useEntry(view, "session");
  return <Menu sessionId={sessionId} commands={view.toJSON().capabilities?.commands ?? NONE} />;
}

/** The new session page's: the host's cached capabilities of the agent picked, once `/` is typed. */
function Cached({ agent }: { agent: string }) {
  const { client } = useHost();
  const ui = useUi();
  const typed = useStore(ui, (state) => state.drafts[NEW_DRAFT]?.startsWith("/") ?? false);
  const { data } = useQuery({
    queryKey: ["agents/capabilities", agent],
    queryFn: () => client.request<AgentsCapabilitiesResult>("agents/capabilities", { agent }),
    enabled: typed,
  });
  return <Menu commands={data?.cached?.capabilities.commands ?? NONE} />;
}

function SlashCommands({ sessionId, agent }: { sessionId?: string; agent: string }) {
  if (sessionId === undefined) return <Cached agent={agent} />;
  return (
    <WithSession sessionId={sessionId}>
      {(view, id) => <Live view={view} sessionId={id} />}
    </WithSession>
  );
}

const extension: ComposerExtension = { side: "left", order: 6, Component: SlashCommands };
export default extension;
