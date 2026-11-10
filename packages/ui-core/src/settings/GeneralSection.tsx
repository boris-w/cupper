// General (design 5.7.4): the composer's keys, kept by the shell as UI
// preferences, and keeping the Mac awake, a host setting.

import { useShell, useT } from "../app/providers";
import { ChevronDownIcon } from "../components/icons";
import { Menu, MenuItem } from "../components/Menu";
import { SEND_KEY_PREF, type SendKey } from "../composer/keys";
import { usePref } from "../composer/send";
import { BUTTON, Row, Section, SwitchRow, useSettings } from "./parts";

const SEND_KEYS: readonly SendKey[] = ["enter", "mod_enter"];

export function GeneralSection() {
  const t = useT();
  const { api } = useShell();
  const { settings, update } = useSettings();
  const sendKey: SendKey = usePref(SEND_KEY_PREF) === "mod_enter" ? "mod_enter" : "enter";
  const trigger = (
    <button type="button" className={`${BUTTON} flex items-center gap-1`}>
      {t(`settings:send_key_${sendKey}`)}
      <ChevronDownIcon />
    </button>
  );
  return (
    <>
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
