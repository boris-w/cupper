// Each privileged action (design 5.7.3): its content from the host, the
// native dialog, and on confirmation its command, all on the control
// connection. A command `policy/describe` describes goes out with the same
// `command_id` it was described with.

import { RpcFailure, uuidv7 } from "@cupper/host-client";
import type {
  AuthElevationGetResult,
  InteractionGetResult,
  InteractionOption,
  PolicyDescribeResult,
  ProjectAddResult,
  ProjectTrustInfoResult,
  SessionCreateResult,
  SessionGetResult,
} from "@cupper/protocol";
import type { PrivilegedAction, PrivilegedResult } from "@cupper/shell-api";
import type { MessageBoxOptions } from "electron";

import type { Translate } from "../host/dialogs";
import * as box from "./dialogs";
import type { PrivilegedHost } from "./host";

/** The dialogs, taken from `dialog` when called (design 5.7.2). */
export interface PrivilegedUi {
  confirm(options: MessageBoxOptions): Promise<{ response: number; checkboxChecked: boolean }>;
  pickDirectory(): Promise<string | null>;
}

interface Cx {
  host: PrivilegedHost;
  ui: PrivilegedUi;
  t: Translate;
}

const CONFIRMED: PrivilegedResult = { status: "confirmed" };
const CANCELLED: PrivilegedResult = { status: "cancelled" };
const failed = (code: string): PrivilegedResult => ({ status: "failed", error: { code } });

async function asked(cx: Cx, options: MessageBoxOptions) {
  const { response, checkboxChecked } = await cx.ui.confirm(options);
  return { yes: response === 0, checked: checkboxChecked };
}

/** A command with its `command_id`, and what the host says it would do. */
async function described(cx: Cx, method: string, params: Record<string, unknown>) {
  const command = { command_id: uuidv7(), ...params };
  const description = await cx.host.request<PolicyDescribeResult>("policy/describe", {
    method,
    params: command,
  });
  const text = (key: string) => {
    const value = description.details[key];
    return typeof value === "string" ? value : undefined;
  };
  return { command, mode: text("mode"), projectPath: text("project_path") };
}

async function title(cx: Cx, sessionId: string): Promise<string> {
  const got = await cx.host.request<SessionGetResult>("session/get", { session_id: sessionId });
  return got.summary.title;
}

export function run(action: PrivilegedAction, cx: Cx): Promise<PrivilegedResult> {
  switch (action.kind) {
    case "interaction":
      return interaction(cx, action);
    case "set_config":
      return setConfig(cx, action);
    case "confirm_config":
      return confirmConfig(cx, action.session_id);
    case "create_session":
      return createSession(cx, action.params as unknown as Record<string, unknown>);
    case "trust_project":
      return trust(cx, action.project_id).then((trusted) => (trusted ? CONFIRMED : CANCELLED));
    case "add_project":
      return addProject(cx);
    case "elevation":
      return elevation(cx, action.elevation_id);
    default:
      // `settings/update` has no handler on the host before T036.
      return Promise.resolve(failed("unsupported"));
  }
}

async function interaction(
  cx: Cx,
  action: Extract<PrivilegedAction, { kind: "interaction" }>,
): Promise<PrivilegedResult> {
  const { session_id, request_id, option_id } = action;
  const got = await cx.host.request<InteractionGetResult>("interaction/get", {
    session_id,
    request_id,
  });
  const { request, digest } = got.requested;
  const listed = "options" in request && Array.isArray(request.options) ? request.options : [];
  const options = listed as InteractionOption[];
  const option = options.find((o) => o.option_id === option_id);
  if (option === undefined) {
    return failed("no_such_option");
  }
  const { command, mode } = await described(cx, "interaction/respond", {
    session_id,
    request_id,
    request_digest: digest,
    outcome: { type: "selected", option_id },
    input: { kind: "native_confirm" },
  });
  const heading = "title" in request && typeof request.title === "string" ? request.title : "";
  if (!(await asked(cx, box.interactionBox(cx.t, heading, option, mode))).yes) {
    return CANCELLED;
  }
  await cx.host.command("interaction/respond", command);
  return CONFIRMED;
}

async function setConfig(
  cx: Cx,
  { session_id, option, value }: Extract<PrivilegedAction, { kind: "set_config" }>,
): Promise<PrivilegedResult> {
  const { command, mode } = await described(cx, "session/set_config", {
    session_id,
    option,
    value,
  });
  const options = box.setConfigBox(cx.t, await title(cx, session_id), mode ?? value);
  if (!(await asked(cx, options)).yes) {
    return CANCELLED;
  }
  await cx.host.command("session/set_config", command);
  return CONFIRMED;
}

/** The digest and the reason come from the session stream (design 5.7.3, T134 ruling). */
async function confirmConfig(cx: Cx, sessionId: string): Promise<PrivilegedResult> {
  const configured = await cx.host.configured(sessionId);
  const digest = configured?.confirmation_required?.config_digest;
  if (configured === undefined || digest === undefined) {
    return failed("unavailable");
  }
  const params = { session_id: sessionId, config_digest: digest };
  const { command } = await described(cx, "session/confirm_config", params);
  if (!(await asked(cx, box.confirmConfigBox(cx.t, await title(cx, sessionId), configured))).yes) {
    return CANCELLED;
  }
  await cx.host.command("session/confirm_config", command);
  return CONFIRMED;
}

async function createSession(cx: Cx, params: Record<string, unknown>): Promise<PrivilegedResult> {
  const { command, mode, projectPath } = await described(cx, "session/create", params);
  const agent = typeof params.agent === "string" ? params.agent : "";
  if (!(await asked(cx, box.createSessionBox(cx.t, agent, mode ?? "", projectPath))).yes) {
    return CANCELLED;
  }
  const created = await cx.host.command<SessionCreateResult>("session/create", command);
  return { status: "confirmed", result: { session_id: created.session_id } };
}

const digestChanged = (error: unknown) =>
  error instanceof RpcFailure &&
  error.error.data?.code === "conflict" &&
  error.error.data.reason === "digest_mismatch";

/**
 * Asks to trust the project's configuration as the host scans it now, the
 * project named by the directory the host has for it, and trusts it; when it
 * changed in the meantime, asks again about what it is then.
 */
async function trust(cx: Cx, projectId: string): Promise<boolean> {
  const info = () =>
    cx.host.request<ProjectTrustInfoResult>("project/trust_info", { project_id: projectId });
  for (let again = false; ; again = true) {
    const current = await info();
    if (!(await asked(cx, box.trustBox(cx.t, current.path, current, again))).yes) {
      return false;
    }
    try {
      const params = { project_id: projectId, trust_digest: current.trust_digest };
      await cx.host.command("project/trust", params);
      return true;
    } catch (error) {
      if (!digestChanged(error)) {
        throw error;
      }
    }
  }
}

/** Picks the directory, adds it, and asks about its configuration when it has any. */
async function addProject(cx: Cx): Promise<PrivilegedResult> {
  const path = await cx.ui.pickDirectory();
  if (path === null) {
    return CANCELLED;
  }
  const { project_id, trust: scan } = await cx.host.command<ProjectAddResult>("project/add", {
    path,
  });
  // Nothing in the scan: the host trusted it on its own (`auto_empty`).
  const trusted = scan.state === "trusted" || (await trust(cx, project_id));
  return { status: "confirmed", result: { project_id, trusted } };
}

async function elevation(cx: Cx, elevationId: string): Promise<PrivilegedResult> {
  const request = await cx.host.request<AuthElevationGetResult>("auth/elevation_get", {
    elevation_id: elevationId,
  });
  const answer = await asked(cx, box.elevationBox(cx.t, request));
  const scopes = request.scopes.filter((s) => answer.yes && (s !== "approve" || answer.checked));
  const decision = answer.yes ? "grant" : "deny";
  await cx.host.command("auth/elevation_respond", { elevation_id: elevationId, decision, scopes });
  return answer.yes ? CONFIRMED : CANCELLED;
}
