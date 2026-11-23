// The title bar's session (design 5.7.4): its title, "新会话" until it has
// one, and a worktree's branch after it; beside them, the agent's mark
// (AG-3, cupper's own: Codex has none).

import { escapeInvisible } from "@cupper/i18n";
import type { SessionSummary } from "@cupper/protocol";

import { useT } from "../app/providers";

/** The agent's short name in a frame: the title bar's and the sidebar card's. */
export function AgentMark({ agent }: { agent: string }) {
  const t = useT();
  return (
    <span className="shrink-0 rounded-xs border border-border px-1 text-xs text-fg-tertiary">
      {t(`sidebar:agent_${agent}`, { defaultValue: agent })}
    </span>
  );
}

export function SessionTitle({ summary }: { summary?: SessionSummary }) {
  const t = useT();
  if (summary === undefined) {
    return null;
  }
  const branch = summary.workspace_kind === "worktree" ? summary.branch : undefined;
  return (
    <div className="flex min-w-0 items-center gap-2 text-sm">
      <span className="truncate font-medium">
        {escapeInvisible(summary.title) || t("sidebar:new_session")}
      </span>
      {branch && <span className="truncate text-fg-tertiary">· {escapeInvisible(branch)}</span>}
      <AgentMark agent={summary.agent} />
    </div>
  );
}
