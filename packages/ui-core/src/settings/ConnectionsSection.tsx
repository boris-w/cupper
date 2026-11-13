// Connections and devices (design 5.7.4): the host's local clients, with what
// weak authentication means; the phone's devices come with M3. Client names
// are what the clients say: only ever text.

import { escapeInvisible } from "@cupper/i18n";
import type { ClientInfo } from "@cupper/protocol";

import { useT } from "../app/providers";
import { useStreams } from "../state/provider";
import { useEntry } from "../state/stream";
import { known, Row, Section } from "./parts";

function Client({ client }: { client: ClientInfo }) {
  const t = useT();
  const parts = [
    known(t, `settings:principal_${client.principal_kind}`),
    client.role && known(t, `settings:role_${client.role}`),
    known(t, `settings:client_auth_${client.auth}`),
    client.elevated && t("settings:elevated"),
  ];
  return (
    <Row
      label={escapeInvisible(client.client_name)}
      description={parts.filter(Boolean).join(" · ")}
      value={escapeInvisible(client.client_version)}
    />
  );
}

export function ConnectionsSection() {
  const t = useT();
  const { view } = useStreams().index;
  useEntry(view, "clients");
  const clients = view.toJSON().clients?.clients ?? [];
  return (
    <>
      <Section title={t("settings:clients")}>
        {clients.map((client) => (
          <Client key={client.conn_id} client={client} />
        ))}
        {clients.some((client) => client.auth === "weak") && (
          <Row label={t("settings:weak_auth")} description={t("settings:weak_auth_detail")} />
        )}
      </Section>
      <Section title={t("settings:devices")}>
        <Row label={t("settings:devices_none")} description={t("settings:devices_later")} />
      </Section>
    </>
  );
}
