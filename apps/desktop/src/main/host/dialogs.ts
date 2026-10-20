// The native dialogs of quitting and of restarting the host. Each reads
// `dialog.showMessageBox` when it is called, so that e2e can replace it in the
// main process (design 5.7.2); the text comes from @cupper/i18n.

import type { HostStatusResult } from "@cupper/protocol";
import { dialog } from "electron";

export type Translate = (key: string, values?: Record<string, unknown>) => string;

async function ask(options: Electron.MessageBoxOptions): Promise<boolean> {
  const { response } = await dialog.showMessageBox({ ...options, defaultId: 1, cancelId: 1 });
  return response === 0;
}

export function confirmQuit(t: Translate, status: HostStatusResult): Promise<boolean> {
  return ask({
    type: "warning",
    message: t("host:quit_title"),
    detail: t("host:quit_detail", {
      turns: status.running_turns,
      requests: status.pending_interactions,
    }),
    buttons: [t("host:quit"), t("host:cancel")],
  });
}

export function confirmRestart(t: Translate): Promise<boolean> {
  return ask({
    type: "warning",
    message: t("host:restart_title"),
    detail: t("host:restart_detail"),
    buttons: [t("host:restart"), t("host:cancel")],
  });
}

export async function tellRestartRefused(t: Translate): Promise<void> {
  await dialog.showMessageBox({ type: "error", message: t("host:restart_refused") });
}
