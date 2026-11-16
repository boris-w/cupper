// The session's options in the composer (design 5.7.4, CP-4): what the
// agent's capabilities offer, the value now, and setting one. A looser
// permission mode follows the tier rules (design 5.8.2): full access only
// through the native confirmation the main process shows, `elevated` with a
// hint (U6). Before the first message, on the new session page, the options
// are those the host cached for the agent picked (AG-4), and setting one
// only chooses what the session is created with.

import type { SessionView } from "@cupper/host-client";
import type { AgentsCapabilitiesResult, ConfigOption, ConfigValue } from "@cupper/protocol";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, type ComponentProps, type ReactNode } from "react";
import { useStore } from "zustand";

import { useHost, useT } from "../app/providers";
import { Alert } from "../components/Alert";
import { MenuItem, MenuLabel } from "../components/Menu";
import { usePrivileged } from "../feedback/privileged";
import { useToasts } from "../feedback/toaster";
import { useStreams, useUi } from "../state/provider";
import { useEntry } from "../state/stream";
import { codeOf } from "./send";

/** A button of the bottom bar (design 5.7.4): 28 high, 13 px; a menu's trigger. */
export function BarButton({ label, ...props }: ComponentProps<"button"> & { label: string }) {
  return (
    <button
      type="button"
      aria-label={label}
      {...props}
      className="flex h-7 items-center gap-1 rounded-full px-2 text-sm text-fg-secondary outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring data-[state=open]:bg-btn-secondary"
    />
  );
}

/** A value that is neither `normal` nor `elevated` counts as full access. */
export const fullAccess = (value?: ConfigValue) =>
  value !== undefined && value.risk !== "normal" && value.risk !== "elevated";

/** Renders `children` with the stream of session `sessionId`, once it is held. */
export function WithSession(props: {
  sessionId?: string;
  children: (view: SessionView, sessionId: string) => ReactNode;
}) {
  const streams = useStreams();
  useEntry(streams.index.view, "view");
  const view = props.sessionId === undefined ? undefined : streams.session(props.sessionId)?.view;
  return view && props.sessionId !== undefined ? props.children(view, props.sessionId) : null;
}

/** The options the composer shows, and setting one. */
export interface Options {
  /** Option `id`, if the agent has it, and its value now. */
  get(id: string): { option?: ConfigOption; value?: ConfigValue };
  set(option: ConfigOption, value: ConfigValue): void;
}

/** Option `id` of `config` at `value`, else at the agent's own value. */
function pick(config: readonly ConfigOption[] | undefined, id: string, value?: string) {
  const option = config?.find((o) => o.id === id);
  const now = option?.values.find((v) => v.value === value) ? value : option?.current;
  return { option, value: option?.values.find((v) => v.value === now) };
}

/**
 * The agent's capabilities as the host cached them; `agents.updated` makes
 * them stale. An empty cache (first use, design 5.6.7) has `agents/refresh`
 * probe the agent once per connection, and is read again after.
 */
export function useCached(agent: string) {
  const { client } = useHost();
  const queries = useQueryClient();
  const key = ["agents/capabilities", agent];
  const { data } = useQuery({
    queryKey: key,
    queryFn: () => client.request<AgentsCapabilitiesResult>("agents/capabilities", { agent }),
  });
  useQuery({
    queryKey: ["agents/refresh", agent],
    queryFn: async () => {
      await client.command("agents/refresh", { agent });
      await queries.invalidateQueries({ queryKey: key });
      return null;
    },
    enabled: data !== undefined && data.cached === undefined,
  });
  return data?.cached?.capabilities;
}

/**
 * The new session page's options: the cached ones of `agent`, each at the
 * user's choice, else at the agent's own value. `chosen` is what
 * `session/create` takes (T266 adds the default permission under it).
 */
export function useNewOptions(agent: string) {
  const ui = useUi();
  const config = useCached(agent)?.config;
  const chosen = useStore(ui, (state) => state.chosen[agent]);
  const get = (id: string) => pick(config, id, chosen?.[id]);
  const set = (option: ConfigOption, value: ConfigValue) =>
    ui.getState().choose(agent, option.id, value.value);
  const picked = Object.keys(chosen ?? {}).flatMap((id) => {
    const { value } = get(id);
    return value ? [[id, value] as const] : [];
  });
  return { get, set, chosen: Object.fromEntries(picked) };
}

/** Renders `children` with the options of session `sessionId`, or of the new session page. */
export function WithOptions(props: {
  sessionId?: string;
  agent: string;
  children: (options: Options) => ReactNode;
}) {
  if (props.sessionId === undefined) {
    return <NewOptions agent={props.agent}>{props.children}</NewOptions>;
  }
  return (
    <WithSession sessionId={props.sessionId}>
      {(view, id) => (
        <SessionOptions view={view} sessionId={id}>
          {props.children}
        </SessionOptions>
      )}
    </WithSession>
  );
}

function NewOptions(props: { agent: string; children: (options: Options) => ReactNode }) {
  return props.children(useNewOptions(props.agent));
}

function SessionOptions(props: {
  view: SessionView;
  sessionId: string;
  children: (options: Options) => ReactNode;
}) {
  useEntry(props.view, "session");
  const set = useSetOption(props.sessionId);
  const { capabilities, configured } = props.view.toJSON();
  return props.children({
    get: (id) => pick(capabilities?.config, id, configured?.config[id]),
    set: (option, value) => void set(option, value),
  });
}

/** Sets an option: full access through the shell, anything else as a command. */
export function useSetOption(sessionId: string) {
  const { client } = useHost();
  const privileged = usePrivileged();
  const t = useT();
  const { show } = useToasts();
  return useCallback(
    async (option: ConfigOption, value: ConfigValue) => {
      const set = { session_id: sessionId, option: option.id, value: value.value };
      if (option.id === "permission_mode" && fullAccess(value)) {
        return void (await privileged({ kind: "set_config", ...set }));
      }
      await client.command("session/set_config", set).catch((error: unknown) => {
        show({ id: "set_config", text: t("composer:config_failed", { code: codeOf(error) }) });
      });
    },
    [sessionId, client, privileged, show, t],
  );
}

/** A titled group of an option's values, the current one ticked. */
export function Choices(props: {
  title: string;
  option: ConfigOption;
  value?: ConfigValue;
  pick: (value: ConfigValue) => void;
  hint?: (value: ConfigValue) => string | undefined;
}) {
  return (
    <>
      <MenuLabel>{props.title}</MenuLabel>
      {props.option.values.map((value) => (
        <MenuItem
          key={value.value}
          radio
          checked={value.value === props.value?.value}
          onSelect={() => props.pick(value)}
        >
          <span className="flex flex-col">
            {value.name}
            <span className="text-xs text-fg-tertiary">{props.hint?.(value)}</span>
          </span>
        </MenuItem>
      ))}
    </>
  );
}

/** Above the composer: full access is on, or the low-interruption tier. */
export function TierHint(props: { sessionId?: string; agent: string }) {
  return <WithOptions {...props}>{(options) => <TierHintOf options={options} />}</WithOptions>;
}

function TierHintOf({ options }: { options: Options }) {
  const t = useT();
  const { value } = options.get("permission_mode");
  if (fullAccess(value)) {
    return (
      <div className="pb-2">
        <Alert>{t("composer:full_access_on")}</Alert>
      </div>
    );
  }
  return (
    value?.risk === "elevated" && (
      <p role="status" className="pb-2 text-center text-sm text-fg-tertiary">
        {t("composer:elevated_on")}
      </p>
    )
  );
}
