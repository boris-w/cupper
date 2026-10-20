// The native confirmation dialogs of the privileged actions (design 5.7.8):
// the default button cancels or refuses; text from the agent or a requester
// shows verbatim, its invisible characters escaped; the rest is
// @cupper/i18n's `privileged` text.

import { escapeInvisible } from "@cupper/i18n";
import type {
  AuthElevationGetResult,
  Finding,
  InteractionOption,
  SessionConfigured,
  TrustScan,
} from "@cupper/protocol";
import type { MessageBoxOptions } from "electron";

import type { Translate } from "../host/dialogs";

/** The requester's own words, at most this long (design 5.8.4). */
const SELF_REPORTED_CHARS = 200;
/** A path past this many characters keeps its start and its end (T135). */
export const PATH_CHARS = 120;
const PATH_HEAD = 30;

const shown = (text: string) => escapeInvisible(text);
const clipped = (text: string) => shown([...text].slice(0, SELF_REPORTED_CHARS).join(""));
/**
 * A path, its invisible characters escaped. A long one keeps its first
 * characters and its last whole components around an ellipsis: the volume or
 * home it is under, and the directory itself.
 */
export function shownPath(path: string): string {
  const chars = [...path];
  if (chars.length <= PATH_CHARS) {
    return shown(path);
  }
  const tail = chars.slice(PATH_HEAD + 1 - PATH_CHARS).join("");
  const component = tail.indexOf("/");
  const end = component > 0 ? tail.slice(component) : tail;
  return `${shown(chars.slice(0, PATH_HEAD).join(""))}…${shown(end)}`;
}

const lines = (...parts: (string | false | undefined)[]) => parts.filter(Boolean).join("\n");

/** Button 0 confirms; button 1, the default and Esc, does not. */
function box(
  t: Translate,
  message: string,
  detail: string,
  buttons: [string, string],
): MessageBoxOptions {
  const [yes, no] = buttons;
  return { type: "warning", message, detail, buttons: [t(yes), t(no)], defaultId: 1, cancelId: 1 };
}

/** What the option does, by its effect (design 5.3.6); full access in `mode`. */
function effect(t: Translate, option: InteractionOption, mode: string | undefined): string {
  const { scope, rule } = option.effect;
  const parts: (string | false)[] = [];
  if (scope === "session") {
    parts.push(t("effect_session"));
  } else if (scope === "persistent") {
    parts.push(t("effect_persistent"));
    parts.push(rule?.text != null && t("effect_rule", { text: shown(rule.text) }));
    parts.push(rule?.location != null && t("effect_location", { location: shown(rule.location) }));
    parts.push(rule?.text == null && rule?.location == null && t("effect_rule_unknown"));
    parts.push(
      rule?.prefix_risk === "interpreter" &&
        t("prefix_interpreter", { prefix: shown(rule.prefix ?? "") }),
    );
    parts.push(rule?.prefix_risk === "unknown" && t("prefix_unknown"));
  } else if (scope === "mode_switch") {
    parts.push(t("effect_mode_switch", { mode: shown(option.effect.mode_after ?? "") }));
  } else if (scope === "unknown") {
    parts.push(t("effect_unknown"));
  }
  parts.push(mode !== undefined && t("full_access", { mode: shown(mode) }));
  return lines(...parts);
}

export function interactionBox(
  t: Translate,
  title: string,
  option: InteractionOption,
  mode?: string,
) {
  const detail = lines(
    t("interaction_detail", { title: shown(title), option: shown(option.name) }),
    effect(t, option, mode),
  );
  return box(t, t("interaction_title"), detail, ["allow", "deny"]);
}

export function setConfigBox(t: Translate, title: string, mode: string) {
  const detail = lines(
    t("set_config_detail", { title: shown(title) }),
    t("full_access", { mode: shown(mode) }),
  );
  return box(t, t("set_config_title"), detail, ["confirm", "cancel"]);
}

export function createSessionBox(t: Translate, agent: string, mode: string, path?: string) {
  const detail = lines(
    t("create_session_detail", { agent: shown(agent) }),
    path !== undefined && t("project_path", { path: shownPath(path) }),
    t("full_access", { mode: shown(mode) }),
  );
  return box(t, t("create_session_title"), detail, ["confirm", "cancel"]);
}

export function confirmConfigBox(t: Translate, title: string, configured: SessionConfigured) {
  const reason = configured.confirmation_required?.reason ?? "";
  const known = ["full_access_unconfirmed", "tier_above_setter"].includes(reason);
  const config = Object.entries(configured.config).map(([key, value]) => `${key}=${value}`);
  const detail = lines(
    t(known ? `confirm_config_${reason}` : "confirm_config_other", { title: shown(title), reason }),
    t("confirm_config_current", { config: shown(config.join(", ")) }),
  );
  return box(t, t("confirm_config_title"), detail, ["confirm", "cancel"]);
}

function finding(t: Translate, f: Finding): string {
  const line = t("finding", { kind: f.kind, detail: shown(f.detail), path: shown(f.path) });
  const outside =
    f.outside_workspace && t("finding_outside", { target: shown(f.target ?? f.path) });
  return lines(line, outside);
}

/** The findings one by one, links out of the workspace marked; "Do not trust" is the default. */
export function trustBox(
  t: Translate,
  path: string,
  scan: Pick<TrustScan, "findings" | "findings_omitted">,
  again: boolean,
) {
  const omitted = scan.findings_omitted ?? 0;
  const detail = lines(
    again && t("trust_again"),
    t("project_path", { path: shownPath(path) }),
    t("trust_detail"),
    ...scan.findings.map((f) => finding(t, f)),
    omitted > 0 && t("findings_omitted", { count: omitted }),
  );
  return box(t, t("trust_title"), detail, ["trust", "untrust"]);
}

/** What the host verified, then what the requester says; approve has its own box, unchecked. */
export function elevationBox(t: Translate, request: AuthElevationGetResult) {
  const { verified, self_reported: said, scopes } = request;
  const chain = verified.parent_chain.map((p) => `${shown(p.executable)} (${p.pid})`).join(" ← ");
  const detail = lines(
    !verified.bundled_cli && t("elevation_not_bundled"),
    t("elevation_verified"),
    t("elevation_executable", { path: shown(verified.executable) }),
    verified.signing == null
      ? t("elevation_unsigned")
      : t("elevation_signing", { signing: shown(verified.signing) }),
    t("elevation_pid", { pid: verified.pid }),
    chain !== "" && t("elevation_parents", { chain }),
    verified.origin_unverifiable && t("elevation_unverifiable"),
    t("elevation_scopes", { scopes: scopes.join(", ") }),
    t("elevation_self_reported"),
    t("elevation_client", { name: clipped(said.client_name) }),
    said.reason != null && t("elevation_reason", { reason: clipped(said.reason) }),
  );
  const options = box(t, t("elevation_title"), detail, ["allow", "deny"]);
  return scopes.includes("approve")
    ? { ...options, checkboxLabel: t("elevation_approve"), checkboxChecked: false }
    : options;
}
