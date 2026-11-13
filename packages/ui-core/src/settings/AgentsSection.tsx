// The agent status (design 5.7.4, AG-2, AG-5): each agent's state, CLI,
// adapter, login, configuration directory and memory, with a restart; then
// the git tools. Paths, versions and plan names are only ever text.

import type { AgentStatus } from "@cupper/protocol";

import { useHost, useT } from "../app/providers";
import { unavailable } from "../newsession/NewSessionPage";
import { LOGIN } from "../rows/messages";
import { useStreams } from "../state/provider";
import { useEntry } from "../state/stream";
import { CommandButton, known, Row, Section } from "./parts";

function AgentCard({ status, focused }: { status: AgentStatus; focused: boolean }) {
  const t = useT();
  const { client } = useHost();
  // The card shows the states a restart goes through.
  const restart = () => client.command("agents/restart", { agent: status.agent });
  const { cli, adapter, auth } = status;
  const state = known(t, `settings:state_${status.state}`);
  const reason = unavailable(status);
  const runtime = adapter.failure
    ? t("settings:runtime_failure", { failure: known(t, `settings:failure_${adapter.failure}`) })
    : known(t, `settings:runtime_${adapter.runtime}`);
  const install = cli?.install !== "native" && known(t, `settings:install_${cli?.install}`);
  const version = cli && [cli.version, known(t, `settings:compat_${cli.compat}`), install];
  return (
    <Section
      title={t(`sidebar:agent_${status.agent}`)}
      action={<CommandButton name="restart" run={restart} />}
      focused={focused}
    >
      <Row
        label={t("settings:state")}
        description={
          reason && reason !== "unavailable"
            ? t("settings:state_because", { state, reason: t(`composer:agent_${reason}`) })
            : state
        }
      />
      <Row
        label={t("settings:cli")}
        description={cli?.path ?? t("settings:cli_missing")}
        value={version?.filter(Boolean).join(" · ")}
      />
      <Row label={t("settings:adapter")} description={runtime} value={adapter.version} />
      <Row
        label={t("settings:auth")}
        description={known(t, `settings:auth_${auth.state}`)}
        value={[auth.kind, auth.plan].filter(Boolean).join(" · ")}
      />
      {auth.state === "logged_out" && LOGIN[status.agent] && (
        <Row label={t("settings:login")} value={LOGIN[status.agent]} />
      )}
      <Row
        label={t("settings:config_dir")}
        description={known(t, `settings:config_dir_${status.config_dir}`)}
      />
      {status.rss_bytes != null && (
        <Row
          label={t("settings:memory")}
          value={t("settings:memory_mb", { mb: Math.round(status.rss_bytes / 2 ** 20) })}
        />
      )}
    </Section>
  );
}

/** `focus` names the agent whose card to bring into view. */
export function AgentsSection({ focus }: { focus?: string }) {
  const t = useT();
  const { view } = useStreams().index;
  useEntry(view, "agents");
  const agents = view.toJSON().agents;
  if (!agents) {
    return <p className="text-sm text-fg-tertiary">{t("settings:checking")}</p>;
  }
  const version = (found?: string) => found ?? t("settings:not_found");
  return (
    <>
      {agents.agents.map((status) => (
        <AgentCard key={status.agent} status={status} focused={focus === status.agent} />
      ))}
      <Section title={t("settings:git")}>
        <Row label={t("settings:git_version")} value={version(agents.git.version)} />
        <Row label={t("settings:git_lfs")} value={version(agents.git.lfs)} />
      </Section>
    </>
  );
}
