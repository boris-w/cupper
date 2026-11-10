// The command-line tool (design 5.7.4, 5.10.1): a link in `~/.local/bin` to
// the app's own CLI, made by the main process after its native confirmation
// (`install_cli`). On weak authentication the CLI still connects read-only
// by uid, so the page shows in every build.

import { escapeInvisible } from "@cupper/i18n";
import type { PrivilegedResult } from "@cupper/shell-api";
import { useState } from "react";

import { useShell, useT } from "../app/providers";
import { feedbackOf } from "../feedback/privileged";
import { CommandButton, Row, Section } from "./parts";

/** The reasons the main process refuses with, as `unavailable`, and where. */
const REFUSALS = ["outside_applications", "occupied"];

function refusal(result: PrivilegedResult): { reason: string; path: string } | undefined {
  if (result.status !== "failed" || typeof result.error !== "object" || result.error === null) {
    return undefined;
  }
  const { reason, path } = result.error as { reason?: unknown; path?: unknown };
  const known = typeof reason === "string" && REFUSALS.includes(reason);
  return known ? { reason, path: escapeInvisible(String(path)) } : undefined;
}

export function CliSection() {
  const t = useT();
  const { api } = useShell();
  const [link, setLink] = useState<string>();
  /** What the toast says: where the link is, why there is none, or the shell's usual feedback. */
  const install = async () => {
    const result = await api.requestPrivileged({ kind: "install_cli" });
    if (result.status === "confirmed") {
      const made = escapeInvisible(String((result.result as { link?: unknown } | undefined)?.link));
      setLink(made);
      return t("settings:install_cli_done", { link: made });
    }
    const refused = refusal(result);
    if (refused) {
      return t(`settings:install_cli_${refused.reason}`, { path: refused.path });
    }
    const feedback = feedbackOf(result);
    return feedback && t(feedback.key, feedback.values);
  };
  return (
    <Section title={t("settings:cli_section")}>
      <Row
        label={t("settings:cli_label")}
        description={link ? t("settings:cli_installed", { link }) : t("settings:cli_detail")}
      >
        <CommandButton name="install_cli" run={install} />
      </Row>
    </Section>
  );
}
