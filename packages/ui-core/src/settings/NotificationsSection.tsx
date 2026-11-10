// Notifications (design 5.7.4): which of the host's notifications to send.

import { useT } from "../app/providers";
import { Section, SwitchRow, useSettings } from "./parts";

const REASONS = ["needs_action", "turn_completed", "error"] as const;

export function NotificationsSection() {
  const t = useT();
  const { settings, update } = useSettings();
  return (
    <Section title={t("settings:notify_when")}>
      {REASONS.map((reason) => {
        const key = `notifications.${reason}` as const;
        return (
          <SwitchRow
            key={reason}
            label={t(`settings:notify_${reason}`)}
            description={t(`settings:notify_${reason}_detail`)}
            checked={settings?.[key]}
            onChange={(on) => update({ [key]: on })}
          />
        );
      })}
    </Section>
  );
}
