// The new session page (design 5.7.4): the heading names the project, and the
// name picks it; under the composer, the action bar picks the agent (AG-3),
// where the session works, and a new worktree's base. An agent that cannot
// run sends nothing, and the bar says why (AG-4).

import { escapeInvisible } from "@cupper/i18n";
import type { AgentStatus, GitBranchesResult, ProjectUpserted } from "@cupper/protocol";
import { useQuery } from "@tanstack/react-query";
import { use, useState } from "react";

import { SlotsContext } from "../app/slots";
import { useHost, useT } from "../app/providers";
import { useRouter } from "../app/router";
import { ChevronDownIcon } from "../components/icons";
import { Menu, MenuItem, MenuLabel, MenuSeparator } from "../components/Menu";
import { Composer } from "../composer/Composer";
import { fullAccess, useNewOptions } from "../composer/config";
import type { NewSession } from "../composer/send";
import { usePrivileged } from "../feedback/privileged";
import { useStreams } from "../state/provider";
import { useEntry } from "../state/stream";

const AGENTS = ["claude", "codex"];

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

function AgentPicker(props: {
  agent: string;
  setAgent: (agent: string) => void;
  statuses: AgentStatus[];
}) {
  const t = useT();
  const reason = (agent: string) => unavailable(props.statuses.find((s) => s.agent === agent));
  const why = (agent: string, className: string) => {
    const code = reason(agent);
    return code && <span className={className}>{t(`composer:agent_${code}`)}</span>;
  };
  const trigger = (
    <button type="button" aria-label={t("composer:pick_agent")} className={PICKER}>
      {t(`sidebar:agent_${props.agent}`)}
      <ChevronDownIcon />
    </button>
  );
  return (
    <span className="flex items-center gap-1">
      <Menu trigger={trigger} align="start">
        {AGENTS.map((agent) => (
          <MenuItem
            key={agent}
            radio
            checked={agent === props.agent}
            onSelect={() => props.setAgent(agent)}
          >
            {t(`sidebar:agent_${agent}`)}
            {why(agent, "ml-2 text-fg-tertiary")}
          </MenuItem>
        ))}
      </Menu>
      {why(props.agent, "text-fg-tertiary")}
    </span>
  );
}

/** Local, or a new worktree, which a project that is no repository's top cannot have (T234). */
function WorkspacePicker(props: {
  worktree: boolean;
  setWorktree: (worktree: boolean) => void;
  project?: ProjectUpserted;
}) {
  const t = useT();
  const trigger = (
    <button type="button" aria-label={t("composer:pick_workspace")} className={PICKER}>
      {t(props.worktree ? "composer:workspace_worktree" : "composer:workspace_local")}
      <ChevronDownIcon />
    </button>
  );
  return (
    <Menu trigger={trigger} align="start">
      <MenuItem radio checked={!props.worktree} onSelect={() => props.setWorktree(false)}>
        {t("composer:workspace_local")}
      </MenuItem>
      <MenuItem
        radio
        checked={props.worktree}
        disabled={!props.project?.is_git}
        onSelect={() => props.setWorktree(true)}
      >
        <span className="flex flex-col">
          {t("composer:workspace_worktree")}
          {!props.project?.is_git && (
            <span className="text-xs text-fg-tertiary">{t("composer:worktree_not_git")}</span>
          )}
        </span>
      </MenuItem>
    </Menu>
  );
}

/** The new worktree's base, of the repository's branches (T124: picked, never typed). */
function BranchPicker(props: {
  branches: GitBranchesResult;
  base: string;
  setBase: (base: string) => void;
}) {
  const t = useT();
  const trigger = (
    <button type="button" aria-label={t("composer:pick_branch")} className={PICKER}>
      {escapeInvisible(props.base)}
      <ChevronDownIcon />
    </button>
  );
  const group = (title: string, branches: string[]) =>
    branches.length > 0 && (
      <>
        <MenuLabel>{title}</MenuLabel>
        {branches.map((branch) => (
          <MenuItem
            key={branch}
            radio
            checked={branch === props.base}
            onSelect={() => props.setBase(branch)}
          >
            {escapeInvisible(branch)}
          </MenuItem>
        ))}
      </>
    );
  return (
    <Menu trigger={trigger} align="start">
      {group(t("composer:branches_local"), props.branches.local)}
      {group(t("composer:branches_remote"), props.branches.remote)}
    </Menu>
  );
}

/** The composer's top sits at 42% of the window's height. */
export function NewSessionPage() {
  const { newSessionComposer } = use(SlotsContext);
  const t = useT();
  const { client } = useHost();
  const { route } = useRouter();
  const index = useStreams().index.view;
  useEntry(index, "view");
  const { projects, agents } = index.toJSON();
  const sorted = Object.values(projects).toSorted((a, b) => a.position - b.position);
  const projectId = (route.page === "new" && route.projectId) || sorted[0]?.project_id;
  const project = projectId === undefined ? undefined : projects[projectId];
  const [agent, setAgent] = useState("claude");
  const [wantWorktree, setWorktree] = useState(false);
  const worktree = wantWorktree && project?.is_git === true;
  const [base, setBase] = useState<{ projectId?: string; branch: string }>();
  const { data: branches } = useQuery({
    queryKey: ["git/branches", projectId],
    queryFn: () => client.request<GitBranchesResult>("git/branches", { project_id: projectId }),
    enabled: worktree,
  });
  const baseRef =
    (base?.projectId === projectId ? base?.branch : undefined) ??
    branches?.current ??
    branches?.local[0];
  const statuses = agents?.agents ?? [];
  const { chosen } = useNewOptions(agent);
  const create: NewSession = {
    agent,
    workspace: worktree && baseRef ? { kind: "worktree", base_ref: baseRef } : { kind: "local" },
    config: Object.fromEntries(Object.entries(chosen).map(([id, value]) => [id, value.value])),
    fullAccess: fullAccess(chosen.permission_mode),
    blocked:
      unavailable(statuses.find((s) => s.agent === agent)) !== undefined || (worktree && !baseRef),
  };
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
      <div className="mt-2 flex items-center gap-3 rounded-xl bg-composer-bar px-2 py-1 text-sm text-fg-secondary">
        <AgentPicker agent={agent} setAgent={setAgent} statuses={statuses} />
        <WorkspacePicker worktree={worktree} setWorktree={setWorktree} project={project} />
        {worktree && branches && baseRef && (
          <BranchPicker
            branches={branches}
            base={baseRef}
            setBase={(branch) => setBase({ projectId, branch })}
          />
        )}
      </div>
    </div>
  );
}
