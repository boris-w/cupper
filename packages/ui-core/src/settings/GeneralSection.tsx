// General (design 5.7.4): each agent's default permission and keeping the
// Mac awake, host settings; the composer's keys, kept by the shell as UI
// preferences.

import type { SettingsPatch } from "@cupper/protocol";

import { useShell, useT } from "../app/providers";
import { ChevronDownIcon } from "../components/icons";
import { Menu, MenuItem } from "../components/Menu";
import { permissionKey, useCached, useDefaultPermission } from "../composer/config";
import { FOLLOW_UP_QUEUE_PREF, SEND_KEY_PREF, type SendKey } from "../composer/keys";
import { usePref } from "../composer/send";
import { BUTTON, Row, Section, SwitchRow, useSettings } from "./parts";

const SEND_KEYS: readonly SendKey[] = ["enter", "mod_enter"];

/**
 * The mode the new session page preselects for `agent` (Q44), of its cached
 * capabilities; the page starts no adapter to fill an empty cache. A mode
 * the agent no longer has follows the CLI, and the row says so.
 */
function DefaultPermission({ agent }: { agent: string }) {
  const t = useT();
  const { update } = useSettings();
  const stored = useDefaultPermission(agent);
  const option = useCached(agent, false)?.config.find((o) => o.id === "permission_mode");
  const value = option?.values.find((v) => v.value === stored);
  const named = { agent: t(`sidebar:agent_${agent}`), mode: stored };
  const lost = option !== undefined && stored !== undefined && value === undefined;
  const why = !option ? "empty" : lost ? "lost" : "detail";
  const set = (mode: string | null) => update({ [permissionKey(agent)]: mode } as SettingsPatch);
  const trigger = (
    <button type="button" className={`${BUTTON} flex items-center gap-1`}>
      {value?.name ?? (option ? undefined : stored) ?? t("settings:follow_cli")}
      <ChevronDownIcon />
    </button>
  );
  return (
    <Row
      label={t("settings:default_permission", named)}
      description={t(`settings:default_permission_${why}`, named)}
    >
      <Menu trigger={trigger}>
        <MenuItem radio checked={value === undefined} onSelect={() => set(null)}>
          {t("settings:follow_cli")}
        </MenuItem>
        {option?.values.map((mode) => (
          <MenuItem
            key={mode.value}
            radio
            checked={mode.value === value?.value}
            onSelect={() => set(mode.value)}
          >
            {mode.name}
          </MenuItem>
        ))}
      </Menu>
    </Row>
  );
}

export function GeneralSection() {
  const t = useT();
  const { api } = useShell();
  const { settings, update } = useSettings();
  const sendKey: SendKey = usePref(SEND_KEY_PREF) === "mod_enter" ? "mod_enter" : "enter";
  const followUpQueue = usePref(FOLLOW_UP_QUEUE_PREF) === true;
  const trigger = (
    <button type="button" className={`${BUTTON} flex items-center gap-1`}>
      {t(`settings:send_key_${sendKey}`)}
      <ChevronDownIcon />
    </button>
  );
  return (
    <>
      <Section title={t("settings:new_session")}>
        <DefaultPermission agent="claude" />
        <DefaultPermission agent="codex" />
      </Section>
      <Section title={t("settings:composer")}>
        <Row label={t("settings:send_key")} description={t(`settings:send_key_${sendKey}_detail`)}>
          <Menu trigger={trigger}>
            {SEND_KEYS.map((key) => (
              <MenuItem
                key={key}
                radio
                checked={key === sendKey}
                onSelect={() => void api.prefs.set(SEND_KEY_PREF, key)}
              >
                {t(`settings:send_key_${key}`)}
              </MenuItem>
            ))}
          </Menu>
        </Row>
        <SwitchRow
          label={t("settings:follow_up_queue")}
          description={t("settings:follow_up_queue_detail")}
          checked={followUpQueue}
          onChange={(on) => void api.prefs.set(FOLLOW_UP_QUEUE_PREF, on)}
        />
      </Section>
      <Section title={t("settings:power")}>
        <SwitchRow
          label={t("settings:prevent_idle_sleep")}
          description={t("settings:prevent_idle_sleep_detail")}
          checked={settings && (settings["host.prevent_idle_sleep"] ?? true)}
          onChange={(on) => update({ "host.prevent_idle_sleep": on })}
        />
      </Section>
    </>
  );
}
