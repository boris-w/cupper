import { HostClient } from "@cupper/host-client";
import { hostError, MockHost } from "@cupper/host-client/mock";
import { createI18n } from "@cupper/i18n";
import type { InputEvidence } from "@cupper/protocol";
import type { MessageBoxOptions } from "electron";
import { describe, expect, test } from "vitest";

import { fixtures, mockConnect } from "../bridge/testing";
import { PATH_CHARS, shownPath } from "./dialogs";
import { controlHost, COOLDOWN_RESET_MS, Privileged, type PrivilegedUi } from "./index";

const ENTER: InputEvidence = { kind: "key", key: "enter" };
const text = createI18n();
const t = (key: string, values?: Record<string, unknown>) => text.t(`privileged:${key}`, values);
const invisible = String.fromCodePoint(0x202e);

/** A Privileged on a client of a mock host; the dialogs answer from `answers`, then cancel. */
async function setup() {
  const host = new MockHost(fixtures);
  const conn = await mockConnect(host);
  const client = new HostClient();
  client.attach(conn.port, conn.handshake, conn.rest);
  const clock = { now: 0 };
  const shown: MessageBoxOptions[] = [];
  const answers: { response: number; checkboxChecked?: boolean }[] = [];
  const ui: PrivilegedUi & { picked: string | null } = {
    picked: "/work/app",
    confirm: async (options) => {
      shown.push(options);
      const { response, checkboxChecked = false } = answers.shift() ?? { response: 1 };
      return { response, checkboxChecked };
    },
    pickDirectory: async () => ui.picked,
  };
  const never = () => new Promise<void>(() => undefined);
  const privileged = new Privileged({ now: () => clock.now, host: controlHost(client, never), t });
  const sent = (method: string) =>
    host.requests
      .filter((r) => r.method === method)
      .map((r) => r.params as Record<string, unknown>);
  /** With Enter, or `null` for no input. */
  const ask = (action: unknown, input: InputEvidence | null = ENTER) =>
    privileged.request(input ?? undefined, action, ui);
  return { host, conn, clock, shown, answers, ui, ask, sent };
}

describe("UNIT-privileged", () => {
  test("needs real input that is not Escape, and opens one dialog at a time", async () => {
    const { ask, ui } = await setup();
    const trust = { kind: "trust_project", project_id: "p_1" };
    expect(await ask(trust, null)).toEqual({ status: "user_activation_required" });
    expect(await ask(trust, { kind: "key", key: "escape" })).toEqual({
      status: "user_activation_required",
    });
    let answer: (value: { response: number; checkboxChecked: boolean }) => void = () => undefined;
    let shown: () => void = () => undefined;
    const open = new Promise<void>((resolve) => (shown = resolve));
    ui.confirm = () => new Promise((resolve) => ((answer = resolve), shown()));
    const first = ask(trust);
    // The second request comes while the first one's dialog is open.
    await open;
    expect(await ask({ kind: "add_project" })).toEqual({ status: "busy" });
    answer({ response: 1, checkboxChecked: false });
    expect(await first).toEqual({ status: "cancelled" });
  });

  test("a cancel holds the same kind back 2 s, 10 s, then 60 s, until a confirmation or 10 minutes", async () => {
    const { ask, clock, answers } = await setup();
    const trust = { kind: "trust_project", project_id: "p_1" };
    for (const wait of [2_000, 10_000, 60_000, 60_000]) {
      expect(await ask(trust)).toEqual({ status: "cancelled" });
      expect(await ask(trust)).toEqual({ status: "cooldown", retry_after_ms: wait });
      expect((await ask({ kind: "settings", patch: {} })).status, "another kind").toBe("failed");
      clock.now += wait;
    }
    expect(await ask(trust)).toEqual({ status: "cancelled" });
    clock.now += COOLDOWN_RESET_MS;
    expect(await ask(trust)).toEqual({ status: "cancelled" });
    expect(await ask(trust)).toEqual({ status: "cooldown", retry_after_ms: 2_000 });
    clock.now += 2_000;
    answers.push({ response: 0 });
    expect(await ask(trust)).toEqual({ status: "confirmed" });
    expect(await ask(trust)).toEqual({ status: "cancelled" });
    expect(await ask(trust)).toEqual({ status: "cooldown", retry_after_ms: 2_000 });
  });

  test("interaction: the request and option verbatim, escaped; deny by default; respond as described", async () => {
    const { host, ask, shown, answers, sent } = await setup();
    const option = {
      option_id: "allow-always",
      name: `always${invisible}`,
      kind: "allow_always",
      requires: "privileged",
      effect: {
        scope: "persistent",
        persistent: true,
        rule: { prefix: "bash", prefix_risk: "interpreter" },
      },
    };
    host.handle("interaction/get", () => {
      const got = structuredClone(fixtures.find((f) => f.method === "interaction/get")?.result) as {
        requested: { request: { options: unknown[] } };
      };
      got.requested.request.options.push(option);
      return { result: got };
    });
    const action = {
      kind: "interaction",
      session_id: "s_1",
      request_id: "r_1",
      option_id: "allow-always",
    };
    answers.push({ response: 0 });
    expect(await ask(action)).toEqual({ status: "confirmed" });
    expect(shown[0]).toMatchObject({ buttons: ["允许", "拒绝"], defaultId: 1, cancelId: 1 });
    expect(shown[0]?.detail).toContain("Run cargo test");
    expect(shown[0]?.detail).toContain("always\\u{202E}");
    expect(shown[0]?.detail).toContain(t("effect_rule_unknown"));
    expect(shown[0]?.detail).toContain(t("prefix_interpreter", { prefix: "bash" }));
    const [described] = sent("policy/describe") as { params: Record<string, unknown> }[];
    const [responded] = sent("interaction/respond");
    expect(responded).toMatchObject({
      outcome: { option_id: "allow-always" },
      input: { kind: "native_confirm" },
    });
    expect(responded?.command_id).toBe(described?.params.command_id);
    expect(await ask({ ...action, option_id: "nope" })).toEqual({
      status: "failed",
      error: { code: "no_such_option" },
    });
  });

  test("set_config and create_session say the mode the host describes", async () => {
    const { host, ask, shown, answers, sent } = await setup();
    host.handle("policy/describe", () => ({
      result: { action: "full_access", details: { session_id: "s_1", mode: "bypassPermissions" } },
    }));
    const config = {
      kind: "set_config",
      session_id: "s_1",
      option: "permission_mode",
      value: "bypassPermissions",
    };
    expect(await ask(config)).toEqual({ status: "cancelled" });
    expect(sent("session/set_config")).toEqual([]);
    expect(shown[0]?.detail).toContain("Fix the build");
    expect(shown[0]?.detail).toContain(t("full_access", { mode: "bypassPermissions" }));
    answers.push({ response: 0 });
    const params = {
      agent: "claude",
      project_id: "p_1",
      message: [],
      workspace: { kind: "local" },
    };
    expect(await ask({ kind: "create_session", params })).toEqual({
      status: "confirmed",
      result: { session_id: "s_1" },
    });
    expect(shown[1]).toMatchObject({ defaultId: 1, message: t("create_session_title") });
    expect(sent("session/create")[0]).toMatchObject({ agent: "claude", project_id: "p_1" });
  });

  test("confirm_config reads the digest and the reason from the session stream", async () => {
    const { host, ask, shown, answers, sent } = await setup();
    const configured = {
      agent: "claude",
      config: { model: "opus", permission_mode: "bypassPermissions" },
      confirmation_required: {
        config_digest: `d_${"1".repeat(64)}`,
        reason: "full_access_unconfirmed",
      },
      project_config: "ignored",
      read_only: false,
      workspace: { base_ref: "main", branch: "cupper/fix", kind: "worktree", path: "/w/fix" },
    };
    host.handle("stream/subscribe", ({ conn }) => {
      const stream = "session/s_1";
      const page = { stream, epoch: 1, at_seq: 1, page: 0, last: true };
      const ops = [{ type: "session.configured", data: configured }];
      queueMicrotask(() =>
        conn.send({ jsonrpc: "2.0", method: "stream/snapshot", params: { ...page, ops } }),
      );
      return { result: { stream, epoch: 1, at_seq: 1, head_seq: 1, mode: "snapshot" } };
    });
    answers.push({ response: 0 });
    expect(await ask({ kind: "confirm_config", session_id: "s_1" })).toEqual({
      status: "confirmed",
    });
    expect(shown[0]?.detail).toContain(
      t("confirm_config_full_access_unconfirmed", { title: "Fix the build" }),
    );
    expect(shown[0]?.detail).toContain("permission_mode=bypassPermissions");
    expect(sent("session/confirm_config")[0]).toMatchObject({
      config_digest: `d_${"1".repeat(64)}`,
    });
    // No snapshot, no dialog.
    host.handle("stream/subscribe", () => hostError("not_found"));
    expect(await ask({ kind: "confirm_config", session_id: "s_2" })).toEqual({
      status: "failed",
      error: { code: "unavailable" },
    });
    expect(shown).toHaveLength(1);
  });

  test("elevation: verified facts first, the requester's own words after; approve unchecked", async () => {
    const { host, ask, shown, answers, sent } = await setup();
    host.handle("auth/elevation_get", () => ({
      result: {
        scopes: ["write", "approve"],
        self_reported: { client_name: `cli${invisible}`, reason: "x".repeat(300) },
        verified: {
          executable: "/usr/local/bin/cupper",
          has_tty: false,
          origin_unverifiable: true,
          parent_chain: [],
          pid: 42,
        },
      },
    }));
    answers.push({ response: 0, checkboxChecked: false });
    expect(await ask({ kind: "elevation", elevation_id: "e_1" })).toEqual({ status: "confirmed" });
    const options = shown[0];
    expect(options).toMatchObject({
      buttons: ["允许", "拒绝"],
      defaultId: 1,
      checkboxChecked: false,
    });
    const detail = options?.detail ?? "";
    expect(detail.indexOf(t("elevation_unsigned"))).toBeLessThan(
      detail.indexOf(t("elevation_self_reported")),
    );
    expect(detail).toContain(t("elevation_unverifiable"));
    expect(detail).toContain("cli\\u{202E}");
    expect(detail).toContain(`${"x".repeat(200)}\n`.trimEnd());
    expect(detail).not.toContain("x".repeat(201));
    expect(sent("auth/elevation_respond")).toEqual([
      expect.objectContaining({ decision: "grant", scopes: ["write"] }),
    ]);
    expect(await ask({ kind: "elevation", elevation_id: "e_1" })).toEqual({ status: "cancelled" });
    expect(sent("auth/elevation_respond")[1]).toMatchObject({ decision: "deny", scopes: [] });
  });

  test("add_project: auto_empty, trust, and asking again after digest_mismatch", async () => {
    const { host, ui, ask, shown, answers, sent } = await setup();
    const added = fixtures.find((f) => f.method === "project/add")?.result as {
      trust: Record<string, unknown>;
    };
    host.handle("project/add", () => ({
      result: { project_id: "p_9", trust: { ...added.trust, state: "trusted", findings: [] } },
    }));
    expect(await ask({ kind: "add_project" })).toEqual({
      status: "confirmed",
      result: { project_id: "p_9", trusted: true },
    });
    expect(shown).toHaveLength(0);

    host.handle("project/add", () => ({
      result: { project_id: "p_1", trust: { ...added.trust, state: "untrusted" } },
    }));
    let mismatches = 1;
    host.handle("project/trust", () =>
      mismatches-- > 0
        ? hostError("conflict", "digest_mismatch")
        : { result: { trust_state: "trusted" } },
    );
    answers.push({ response: 0 }, { response: 0 });
    expect(await ask({ kind: "add_project" })).toEqual({
      status: "confirmed",
      result: { project_id: "p_1", trusted: true },
    });
    expect(shown[0]).toMatchObject({ buttons: ["信任", "不信任"], defaultId: 1 });
    expect(shown[0]?.detail).toContain("/work/app");
    expect(shown[0]?.detail).toContain(t("finding_outside", { target: "/work/dotfiles/mcp.json" }));
    expect(shown[1]?.detail).toContain(t("trust_again"));
    expect(sent("project/trust")).toHaveLength(2);
    // Each dialog shows the host's scan of the moment.
    expect(sent("project/trust_info")).toHaveLength(2);

    ui.picked = null;
    expect(await ask({ kind: "add_project" })).toEqual({ status: "cancelled" });
  });

  test("settings has no host method yet, and malformed actions are refused", async () => {
    const { ask } = await setup();
    expect(await ask({ kind: "settings", patch: {} })).toEqual({
      status: "failed",
      error: { code: "unsupported" },
    });
    for (const action of [
      null,
      { kind: "elevation" },
      { kind: "toString" },
      { kind: "create_session", params: 1 },
    ]) {
      expect(await ask(action)).toEqual({ status: "failed", error: { code: "invalid_params" } });
    }
  });
});

describe("UNIT-privileged-paths", () => {
  const fixture = (method: string) =>
    structuredClone(fixtures.find((f) => f.method === method)?.result) as Record<string, unknown>;

  test("trusting a project names it by the directory the host has, escaped", async () => {
    const { host, ask, shown } = await setup();
    host.handle("project/trust_info", () => ({
      result: { ...fixture("project/trust_info"), path: `/work/app${invisible}x` },
    }));
    host.handle("project/add", () => ({
      result: { project_id: "p_1", trust: fixture("project/trust_info") },
    }));
    expect(await ask({ kind: "trust_project", project_id: "p_1" })).toEqual({
      status: "cancelled",
    });
    // add_project too, whatever directory the window's picker said.
    expect((await ask({ kind: "add_project" })).status).toBe("confirmed");
    for (const options of shown) {
      expect(options.detail).toContain(t("project_path", { path: "/work/app\\u{202E}x" }));
    }
    expect(shown).toHaveLength(2);
  });

  test("creating a session names the project by the path the host describes", async () => {
    const { host, ask, shown } = await setup();
    const details = {
      agent: "claude",
      project_id: "p_1",
      project_path: "/work/app",
      mode: "bypassPermissions",
    };
    host.handle("policy/describe", () => ({ result: { action: "full_access", details } }));
    const params = {
      agent: "claude",
      project_id: "p_1",
      message: [],
      workspace: { kind: "local" },
    };
    expect(await ask({ kind: "create_session", params })).toEqual({ status: "cancelled" });
    expect(shown[0]?.detail).toContain(t("project_path", { path: "/work/app" }));
  });

  test("a long path keeps its start and its last whole components", () => {
    const deep = `/Volumes/Work/${"nested-directory/".repeat(12)}the-project`;
    const cut = shownPath(deep);
    expect(cut.startsWith(deep.slice(0, 30))).toBe(true);
    expect(cut).toContain("…/");
    expect(cut.endsWith("/nested-directory/the-project")).toBe(true);
    expect([...cut].length).toBeLessThanOrEqual(PATH_CHARS);
    expect(shownPath("/work/app")).toBe("/work/app");
    expect(shownPath(`/work/${invisible}app`)).toBe("/work/\\u{202E}app");
  });

  test("an elevation from a CLI that is not the app's own says so first", async () => {
    const { host, ask, shown, clock } = await setup();
    for (const bundled_cli of [false, true]) {
      host.handle("auth/elevation_get", () => {
        const got = fixture("auth/elevation_get") as { verified: Record<string, unknown> };
        got.verified.bundled_cli = bundled_cli;
        return { result: got };
      });
      await ask({ kind: "elevation", elevation_id: `e_${bundled_cli}` });
      clock.now += 2_000;
    }
    expect(shown[0]?.detail?.startsWith(t("elevation_not_bundled"))).toBe(true);
    expect(shown[1]?.detail).not.toContain(t("elevation_not_bundled"));
  });
});
