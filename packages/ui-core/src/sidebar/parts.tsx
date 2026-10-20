// The sidebar's parts (design 5.7.4, ui-spec 4.1), drawn from index data in
// props. Titles, names and client names are agent or user strings: they are
// only ever text nodes.

import type { ClientState } from "@cupper/host-client";
import type {
  ElevationRequested,
  HostUpdated,
  ProjectUpserted,
  SessionSummary,
} from "@cupper/protocol";
import { useState, type ComponentProps, type ReactNode } from "react";

import { useT } from "../app/providers";
import { useRouter } from "../app/router";
import {
  ComposeIcon,
  ErrorIcon,
  FolderIcon,
  MoreIcon,
  PlusIcon,
  Spinner,
} from "../components/icons";
import { Menu } from "../components/Menu";
import { Tooltip } from "../components/Tooltip";
import { usePrivileged } from "../feedback/privileged";

/** Sessions a project shows before "展开显示". */
export const SHOWN_SESSIONS = 5;

const DOT: Record<string, string> = {
  ready: "bg-status-connected",
  failed: "bg-status-error",
  closed: "bg-status-disconnected",
};

/** The host's status dot: green, grey or red, a blue spinner while connecting; details in the tooltip. */
export function HostStatus({ state, host }: { state: ClientState; host?: HostUpdated | null }) {
  const t = useT();
  const status = t(`shell:host_${state.status}`);
  const details = [
    status,
    host && t("sidebar:host_version", { version: host.version }),
    host?.pending_upgrade && t("sidebar:host_upgrade"),
  ];
  const dot = DOT[state.status];
  return (
    <Tooltip
      content={<span className="whitespace-pre-line">{details.filter(Boolean).join("\n")}</span>}
    >
      <span
        role="img"
        aria-label={status}
        className="flex size-3 items-center justify-center text-accent"
      >
        {dot ? <span className={`size-2 rounded-full ${dot}`} /> : <Spinner className="size-3" />}
      </span>
    </Tooltip>
  );
}

const ROW =
  "flex h-(--layout-row-h) w-full items-center gap-2 rounded-row px-(--layout-row-px) text-left";

function RowButton({ label, ...props }: ComponentProps<"button"> & { label: string }) {
  return (
    <Tooltip content={label}>
      <button
        type="button"
        aria-label={label}
        {...props}
        className="flex size-5 items-center justify-center rounded-md text-fg-tertiary outline-none hover:bg-active focus-visible:ring-2 focus-visible:ring-ring data-[state=open]:bg-active"
      />
    </Tooltip>
  );
}

/** "新会话", then the projects with their sessions, most recent first. */
export function ProjectList(props: { projects: ProjectUpserted[]; sessions: SessionSummary[] }) {
  const t = useT();
  const { route, navigate } = useRouter();
  const privileged = usePrivileged();
  const current = route.page === "session" ? route.sessionId : undefined;
  const projects = props.projects.toSorted((a, b) => a.position - b.position);
  const sessions = props.sessions.toSorted((a, b) => b.last_activity_at - a.last_activity_at);
  return (
    <nav aria-label={t("shell:sidebar")} className="flex flex-col px-2 text-base">
      <button
        type="button"
        onClick={() => navigate({ page: "new" })}
        className={`${ROW} hover:bg-hover ${route.page === "new" ? "bg-hover" : ""}`}
      >
        <ComposeIcon />
        {t("sidebar:new_session")}
      </button>
      <div className={`${ROW} mt-3 justify-between text-sm text-fg-tertiary`}>
        {t("sidebar:projects")}
        <RowButton
          label={t("sidebar:add_project")}
          onClick={() => void privileged({ kind: "add_project" })}
        >
          <PlusIcon />
        </RowButton>
      </div>
      {projects.map((project) => (
        <ProjectGroup
          key={project.project_id}
          project={project}
          sessions={sessions.filter((session) => session.project_id === project.project_id)}
          current={current}
        />
      ))}
    </nav>
  );
}

function ProjectGroup(props: {
  project: ProjectUpserted;
  sessions: SessionSummary[];
  current?: string;
}) {
  const { project, sessions } = props;
  const t = useT();
  const { navigate } = useRouter();
  const [expanded, setExpanded] = useState(false);
  const shown = expanded ? sessions : sessions.slice(0, SHOWN_SESSIONS);
  return (
    <section aria-label={project.name}>
      <div className={`${ROW} group relative hover:bg-hover`}>
        <FolderIcon />
        <span className="min-w-0 flex-1 truncate">{project.name}</span>
        {/* At the row's end like a session's label; the actions take its place on hover. */}
        {project.trust_state !== "trusted" && (
          <span className="shrink-0 text-xs text-warning group-focus-within:invisible group-hover:invisible group-has-[[data-state=open]]:invisible">
            {t("sidebar:untrusted")}
          </span>
        )}
        {/* The menu's items come with T153. */}
        <span className="absolute right-(--layout-row-px) flex gap-0.5 opacity-0 group-hover:opacity-100 focus-within:opacity-100 has-data-[state=open]:opacity-100">
          <RowButton
            label={t("sidebar:new_session_in", { project: project.name })}
            onClick={() => navigate({ page: "new", projectId: project.project_id })}
          >
            <PlusIcon />
          </RowButton>
          <Menu
            trigger={
              <RowButton label={t("sidebar:project_menu")}>
                <MoreIcon />
              </RowButton>
            }
          />
        </span>
      </div>
      {shown.map((session) => (
        <SessionRow
          key={session.session_id}
          session={session}
          selected={session.session_id === props.current}
        />
      ))}
      {sessions.length > SHOWN_SESSIONS && (
        <button
          type="button"
          onClick={() => setExpanded(!expanded)}
          className={`${ROW} pl-8 text-sm text-fg-tertiary hover:bg-hover`}
        >
          {t(expanded ? "sidebar:show_less" : "sidebar:show_more")}
        </button>
      )}
    </section>
  );
}

/** The state at the start of a session row (design 5.7.4): a spinner, an error, the unread dot. */
function Lead({ session }: { session: SessionSummary }) {
  const t = useT();
  const mark = (label: string, className: string, icon?: ReactNode) => (
    <span role="img" aria-label={label} className={className}>
      {icon}
    </span>
  );
  switch (session.status) {
    case "running":
      return mark(t("sidebar:running"), "text-fg-tertiary", <Spinner className="size-3.5" />);
    case "error": {
      const fallback = t("sidebar:error_other");
      const text = t(`sidebar:error_${session.error_code ?? "other"}`, { defaultValue: fallback });
      return <Tooltip content={text}>{mark(text, "text-danger", <ErrorIcon />)}</Tooltip>;
    }
    case "unread":
      return mark(t("sidebar:unread"), "size-2 rounded-full bg-accent");
    default:
      return null;
  }
}

function SessionRow({ session, selected }: { session: SessionSummary; selected: boolean }) {
  const t = useT();
  const { navigate } = useRouter();
  const tag =
    session.status !== "needs_action"
      ? undefined
      : t(
          session.pending_kind === "approval" ? "sidebar:awaiting_approval" : "sidebar:needs_input",
        );
  return (
    <Tooltip content={t(`sidebar:agent_${session.agent}`, { defaultValue: session.agent })}>
      <button
        type="button"
        aria-current={selected ? "page" : undefined}
        onClick={() => navigate({ page: "session", sessionId: session.session_id })}
        className={`${ROW} hover:bg-hover ${selected ? "bg-hover" : ""}`}
      >
        <span className="flex size-4 shrink-0 items-center justify-center">
          <Lead session={session} />
        </span>
        <span className="min-w-0 flex-1 truncate">{session.title}</span>
        {tag && <span className="shrink-0 text-xs text-fg-tertiary">{tag}</span>}
      </button>
    </Tooltip>
  );
}

/** The CLI's elevation requests, oldest first (design 5.12.2); "查看" opens the native confirmation. */
export function ElevationBanner({ elevations }: { elevations: ElevationRequested[] }) {
  const t = useT();
  const privileged = usePrivileged();
  const [first, ...rest] = elevations.toSorted((a, b) => a.requested_at - b.requested_at);
  if (!first) {
    return null;
  }
  return (
    <div
      role="status"
      className="m-2 flex items-center gap-3 rounded-xl bg-surface-panel p-3 text-sm shadow-card"
    >
      <div className="min-w-0 flex-1">
        {/* Generic (design 5.8.4): the requester's own name is unverified; the native dialog shows what the host verified. */}
        <p>{t("sidebar:elevation")}</p>
        {rest.length > 0 && (
          <p className="text-xs text-fg-tertiary">
            {t("sidebar:elevation_more", { count: rest.length })}
          </p>
        )}
      </div>
      <button
        type="button"
        onClick={() => void privileged({ kind: "elevation", elevation_id: first.elevation_id })}
        className="shrink-0 rounded-lg bg-btn-primary px-2.5 py-1 text-btn-primary-fg outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        {t("sidebar:review")}
      </button>
    </div>
  );
}
