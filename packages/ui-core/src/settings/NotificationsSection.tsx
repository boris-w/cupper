// Notifications (design 5.7.4): which of the host's notifications to send,
// and where to allow them once macOS has denied them (T250).

import { useT } from "../app/providers";
import { useStreams } from "../state/provider";
import { useEntry } from "../state/stream";
import { Row, Section, SwitchRow, useSettings } from "./parts";

const REASONS = ["needs_action", "turn_completed", "error"] as const;

export function NotificationsSection() {
  const t = useT();
  const { settings, update } = useSettings();
  const { view } = useStreams().index;
  useEntry(view, "host");
  const denied = view.toJSON().host?.notification_auth === "denied";
  return (
    <Section title={t("settings:notify_when")}>
      {denied && (
        <Row label={t("settings:notify_denied")} description={t("settings:notify_denied_detail")} />
      )}
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
