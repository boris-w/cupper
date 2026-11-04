// The session's options in the composer (design 5.7.4, CP-4): what the
// agent's capabilities offer, the value now, and setting one. A looser
// permission mode follows the tier rules (design 5.8.2): full access only
// through the native confirmation the main process shows, `elevated` with a
// hint (U6).

import type { SessionView } from "@cupper/host-client";
import type { ConfigOption, ConfigValue } from "@cupper/protocol";
import { useCallback, type ComponentProps, type ReactNode } from "react";

import { useHost, useT } from "../app/providers";
import { Alert } from "../components/Alert";
import { MenuItem, MenuLabel } from "../components/Menu";
import { usePrivileged } from "../feedback/privileged";
import { useToasts } from "../feedback/toaster";
import { useStreams } from "../state/provider";
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

/** The session's option `id`, if the agent has it, and its value now. */
export function useOption(view: SessionView, id: string) {
  useEntry(view, "session");
  const { capabilities, configured } = view.toJSON();
  const option = capabilities?.config.find((o) => o.id === id);
  const current = configured?.config[id] ?? option?.current;
  return { option, value: option?.values.find((v) => v.value === current) };
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
export function TierHint({ sessionId }: { sessionId?: string }) {
  return <WithSession sessionId={sessionId}>{(view) => <TierHintOf view={view} />}</WithSession>;
}

function TierHintOf({ view }: { view: SessionView }) {
  const t = useT();
  const { value } = useOption(view, "permission_mode");
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
