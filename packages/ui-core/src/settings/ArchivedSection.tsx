// Archived sessions (design 5.7.4, PS-3): the latest first, a page at a time,
// each with a restore. A restored session goes back to the sidebar and off
// this list; a toast says how it came back. Titles and project names are only
// ever text.

import { escapeInvisible } from "@cupper/i18n";
import type { ArchivedSession, SessionRestoreResult } from "@cupper/protocol";
import { useHost, useT } from "../app/providers";
import { useArchivedSessions } from "../state/query";
import { useStreams } from "../state/provider";
import { useEntry } from "../state/stream";
import { BUTTON, CommandButton, Row, Section } from "./parts";

function Archived({ archived: { summary, archived_at } }: { archived: ArchivedSession }) {
  const t = useT();
  const { client } = useHost();
  const { view } = useStreams().index;
  useEntry(view, `project:${summary.project_id}`);
  const project = view.toJSON().projects[summary.project_id];
  const title = escapeInvisible(summary.title) || t("settings:untitled");
  const when = t("settings:archived_at", { at: new Date(archived_at) });
  // A worktree's restore takes about a second (T241).
  const restore = async () => {
    const params = { session_id: summary.session_id };
    const { mode } = await client.command<SessionRestoreResult>("session/restore", params);
    return t(mode === "read_only" ? "settings:restored_read_only" : "settings:restored", { title });
  };
  return (
    <Row
      label={title}
      description={[project && escapeInvisible(project.name), when].filter(Boolean).join(" · ")}
    >
      <CommandButton name="restore" run={restore} />
    </Row>
  );
}

export function ArchivedSection() {
  const t = useT();
  const { data, isPending, hasNextPage, isFetchingNextPage, fetchNextPage } = useArchivedSessions();
  const sessions = data?.pages.flatMap((page) => page.sessions) ?? [];
  if (isPending || sessions.length === 0) {
    const empty = isPending ? t("settings:loading") : t("settings:archived_none");
    return <p className="text-sm text-fg-tertiary">{empty}</p>;
  }
  return (
    <Section title={t("settings:sessions")}>
      {sessions.map((archived) => (
        <Archived key={archived.summary.session_id} archived={archived} />
      ))}
      {hasNextPage && (
        <div className="flex justify-center px-4 py-2">
          <button
            type="button"
            disabled={isFetchingNextPage}
            onClick={() => void fetchNextPage()}
            className={BUTTON}
          >
            {t("settings:load_more")}
          </button>
        </div>
      )}
    </Section>
  );
}
