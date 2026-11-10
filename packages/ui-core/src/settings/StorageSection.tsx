// Storage (design 5.7.4, 5.4.5): what each kind of the host's data takes, as
// `host/storage` measures it each time the page opens, and clearing the
// snapshot objects. Sizes are only ever text.

import { formatBytes } from "@cupper/i18n";
import type { HostStorageResult } from "@cupper/protocol";
import { useQuery } from "@tanstack/react-query";

import { useHost, useT } from "../app/providers";
import { usePrivileged } from "../feedback/privileged";
import { useToasts } from "../feedback/toaster";
import { CommandButton, Row, Section } from "./parts";

const KINDS = ["host_db", "raw_db", "blobs", "snapshots", "worktrees"] as const;

/** The host refused the purge: a turn runs or gets ready. */
const turnRunning = (error: unknown) =>
  typeof error === "object" &&
  error !== null &&
  "reason" in error &&
  error.reason === "turn_running";

export function StorageSection() {
  const t = useT();
  const { client } = useHost();
  const privileged = usePrivileged();
  const { show } = useToasts();
  const { data, refetch } = useQuery({
    queryKey: ["host/storage"],
    queryFn: () => client.request<HostStorageResult>("host/storage", {}),
    refetchOnMount: "always",
  });
  const purge = async () => {
    const result = await privileged({ kind: "purge_snapshots" });
    if (result.status === "failed" && turnRunning(result.error)) {
      // In place of the shell's toast about the same action.
      show({ id: "privileged:purge_snapshots", text: t("settings:purge_turn_running") });
    }
    if (result.status !== "confirmed") {
      return undefined;
    }
    void refetch();
    return t("settings:purged");
  };
  if (data === undefined) {
    return <p className="text-sm text-fg-tertiary">{t("settings:loading")}</p>;
  }
  const partial = data.partial && (
    <span className="text-sm text-fg-secondary">{t("settings:storage_partial")}</span>
  );
  return (
    <Section title={t("settings:storage")} action={partial}>
      {KINDS.map((kind) => (
        <Row
          key={kind}
          label={t(`settings:storage_${kind}`)}
          description={t(`settings:storage_${kind}_detail`)}
          value={formatBytes(data[kind])}
        >
          {kind === "snapshots" && <CommandButton name="purge" run={purge} />}
        </Row>
      ))}
    </Section>
  );
}
