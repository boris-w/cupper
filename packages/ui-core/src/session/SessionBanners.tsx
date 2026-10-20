// The warning boxes above a session (design 5.7.4, 5.7.5): a project whose
// configuration is not loaded, and a session configuration to confirm. Each
// goes away when the streams say so, never ahead of them.

import type { SessionView } from "@cupper/host-client";
import type { ConfirmationRequired, ProjectUpserted } from "@cupper/protocol";

import { useT } from "../app/providers";
import { Alert } from "../components/Alert";
import { usePrivileged } from "../feedback/privileged";
import { useStreams } from "../state/provider";
import { useEntry } from "../state/stream";

const ACTION =
  "shrink-0 rounded-lg bg-btn-secondary px-2.5 py-1 outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring";

/** Untrusted or stale (design 5.2.6): the project's agent configuration is not loaded. */
export function UntrustedBanner({ project }: { project?: ProjectUpserted }) {
  const t = useT();
  const privileged = usePrivileged();
  if (!project || project.trust_state === "trusted") {
    return null;
  }
  const trust = () => void privileged({ kind: "trust_project", project_id: project.project_id });
  return (
    <Alert
      action={
        <button type="button" onClick={trust} className={ACTION}>
          {t("stream:trust")}
        </button>
      }
    >
      {t(project.trust_state === "stale" ? "stream:stale" : "stream:untrusted")}
    </Alert>
  );
}

/** The session's configuration needs the user's confirmation; the native dialog shows it. */
export function ConfirmBanner(props: {
  sessionId: string;
  confirmation?: ConfirmationRequired | null;
}) {
  const t = useT();
  const privileged = usePrivileged();
  const reason = props.confirmation?.reason;
  if (reason === undefined) {
    return null;
  }
  const confirm = () => void privileged({ kind: "confirm_config", session_id: props.sessionId });
  return (
    <Alert
      action={
        <button type="button" onClick={confirm} className={ACTION}>
          {t("stream:confirm")}
        </button>
      }
    >
      {t(`stream:unconfirmed_${reason}`, {
        reason,
        defaultValue: t("stream:unconfirmed_other", { reason }),
      })}
    </Alert>
  );
}

function ConfirmFromStream({ sessionId, view }: { sessionId: string; view: SessionView }) {
  useEntry(view, "session");
  return (
    <ConfirmBanner
      sessionId={sessionId}
      confirmation={view.toJSON().configured?.confirmation_required}
    />
  );
}

export function SessionBanners({ sessionId }: { sessionId: string }) {
  const streams = useStreams();
  const index = streams.index.view;
  useEntry(index, "view");
  const { sessions, projects } = index.toJSON();
  const projectId = sessions[sessionId]?.project_id;
  const session = streams.session(sessionId)?.view;
  return (
    <div className="flex flex-col items-start gap-(--layout-item-gap-grouped) pt-(--layout-item-gap)">
      <UntrustedBanner project={projectId === undefined ? undefined : projects[projectId]} />
      {session && <ConfirmFromStream sessionId={sessionId} view={session} />}
    </div>
  );
}
