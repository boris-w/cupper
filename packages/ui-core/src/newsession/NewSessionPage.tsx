// The new session page (design 5.7.4): the heading names the project, and the
// name picks it; under the composer, the action bar picks the agent (AG-3)
// and shows where the session works. M1a: Claude only, on the local copy.

import { escapeInvisible } from "@cupper/i18n";
import type { AgentStatus, ProjectUpserted } from "@cupper/protocol";
import { use, useMemo, useState } from "react";

import { SlotsContext } from "../app/pages";
import { useT } from "../app/providers";
import { useRouter } from "../app/router";
import { ChevronDownIcon } from "../components/icons";
import { Menu, MenuItem, MenuSeparator } from "../components/Menu";
import { Composer } from "../composer/Composer";
import { DEFAULT_NEW_SESSION } from "../composer/send";
import { usePrivileged } from "../feedback/privileged";
import { useStreams } from "../state/provider";
import { useEntry } from "../state/stream";

const PICKER =
  "inline-flex items-center gap-1 rounded-lg px-1.5 outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring";

/** Why the agent cannot run, as a code; `undefined` while it can or the host has not said. */
export function unavailable(status?: AgentStatus): string | undefined {
  if (!status || !["unavailable", "crashed"].includes(status.state)) {
    return undefined;
  }
  if (!status.cli) {
    return "cli_missing";
  }
  if (status.auth.state === "logged_out") {
    return "not_logged_in";
  }
  return status.adapter.runtime === "failed" ? "runtime_failed" : "unavailable";
}

function ProjectPicker({
  project,
  projects,
}: {
  project: ProjectUpserted;
  projects: ProjectUpserted[];
}) {
  const t = useT();
  const { navigate } = useRouter();
  const privileged = usePrivileged();
  const trigger = (
    <button type="button" aria-label={t("composer:pick_project")} className={PICKER}>
      {escapeInvisible(project.name)}
      <ChevronDownIcon />
    </button>
  );
  return (
    <Menu trigger={trigger} align="center">
      {projects.map((p) => (
        <MenuItem
          key={p.project_id}
          onSelect={() => navigate({ page: "new", projectId: p.project_id })}
        >
          {escapeInvisible(p.name)}
          {p.trust_state !== "trusted" && (
            <span className="ml-2 text-fg-tertiary">{t("sidebar:untrusted")}</span>
          )}
        </MenuItem>
      ))}
      <MenuSeparator />
      <MenuItem onSelect={() => void privileged({ kind: "add_project" })}>
        {t("sidebar:add_project")}
      </MenuItem>
    </Menu>
  );
}

function ActionBar(props: {
  agent: string;
  setAgent: (agent: string) => void;
  status?: AgentStatus;
}) {
  const t = useT();
  const reason = unavailable(props.status);
  const trigger = (
    <button type="button" aria-label={t("composer:pick_agent")} className={PICKER}>
      {t(`sidebar:agent_${props.agent}`)}
      <ChevronDownIcon />
    </button>
  );
  return (
    <div className="mt-2 flex items-center gap-3 rounded-xl bg-composer-bar px-2 py-1 text-sm text-fg-secondary">
      <Menu trigger={trigger} align="start">
        <MenuItem onSelect={() => props.setAgent("claude")}>
          {t("sidebar:agent_claude")}
          {reason && <span className="ml-2 text-fg-tertiary">{t(`composer:agent_${reason}`)}</span>}
        </MenuItem>
      </Menu>
      <span>{t("composer:workspace_local")}</span>
    </div>
  );
}

/** The composer's top sits at 42% of the window's height. */
export function NewSessionPage() {
  const { newSessionComposer } = use(SlotsContext);
  const t = useT();
  const { route } = useRouter();
  const index = useStreams().index.view;
  useEntry(index, "view");
  const { projects, agents } = index.toJSON();
  const sorted = Object.values(projects).toSorted((a, b) => a.position - b.position);
  const projectId = (route.page === "new" && route.projectId) || sorted[0]?.project_id;
  const project = projectId === undefined ? undefined : projects[projectId];
  const [agent, setAgent] = useState(DEFAULT_NEW_SESSION.agent);
  const create = useMemo(() => ({ ...DEFAULT_NEW_SESSION, agent }), [agent]);
  return (
    <div className="mx-auto w-full max-w-(--layout-thread-max-w) px-(--layout-thread-gutter) pt-[calc(var(--layout-home-composer-anchor)-var(--layout-titlebar-h)-6rem)]">
      <h1 className="mb-8 flex h-16 items-center justify-center gap-1.5 text-heading-app font-medium">
        {project ? (
          <>
            {t("composer:heading_before")}
            <ProjectPicker project={project} projects={sorted} />
            {t("composer:heading_after")}
          </>
        ) : (
          t("composer:heading_none")
        )}
      </h1>
      {newSessionComposer ?? <Composer create={create} />}
      <ActionBar
        agent={agent}
        setAgent={setAgent}
        status={agents?.agents.find((a) => a.agent === agent)}
      />
    </div>
  );
}
