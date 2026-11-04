// The review panel's "Git 操作" (design 5.7.5, 5.2.10): commit, commit and
// push, or push, from one dialog. These run in the user tier: the repository's
// hooks, signing and credential helpers run as in a terminal, which the dialog
// says, with the paths of any executable configuration changed this session.
// Failures are guided by their semantic code; git's own output shows as text.

import { RpcFailure } from "@cupper/host-client";
import type { GitDiffResult, GitStatusResult } from "@cupper/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState } from "react";

import { useHost, useT } from "../app/providers";
import { Alert } from "../components/Alert";
import { ChevronDownIcon } from "../components/icons";
import { Menu, MenuItem } from "../components/Menu";
import { ToolbarButton } from "../components/ToolbarButton";
import { useToasts } from "../feedback/toaster";
import { useStatus } from "./state";

type Step = "commit" | "commit_push" | "push";
const STEPS: Step[] = ["commit", "commit_push", "push"];
/** The reasons with their own guidance (T123); others say their code. */
const GUIDED = [
  ...["signing_failed", "auth_failed", "hook_failed", "nothing_to_commit"],
  ...["rejected", "lock_held", "outcome_unknown"],
];

interface Failure {
  key: string;
  code: string;
  /** The commit failed after "包含未暂存的更改" staged the changes. */
  staged: boolean;
  stderr?: string;
}

function failureOf(error: unknown, staged: boolean): Failure {
  const data = error instanceof RpcFailure ? error.error.data : undefined;
  const { code, reason, stderr } = (data ?? {}) as {
    code?: string;
    reason?: string;
    stderr?: unknown;
  };
  const guided = reason !== undefined && GUIDED.includes(reason);
  return {
    key: guided ? `review:git_${reason}` : "review:failed",
    code: reason ?? code ?? "unexpected",
    staged,
    stderr: typeof stderr === "string" && stderr !== "" ? stderr : undefined,
  };
}

export function GitActions({ sessionId }: { sessionId: string }) {
  const t = useT();
  const status = useStatus(sessionId).data;
  const [step, setStep] = useState<Step>();
  const trigger = useRef<HTMLButtonElement>(null);
  return (
    <>
      <Menu
        trigger={
          <ToolbarButton ref={trigger} label={t("review:git_actions")}>
            {t("review:git_actions")}
            <ChevronDownIcon />
          </ToolbarButton>
        }
      >
        <MenuItem disabled={!status} onSelect={() => setStep("commit")}>
          {t("review:git_commit_menu")}
        </MenuItem>
        <MenuItem disabled={status?.branch === undefined} onSelect={() => setStep("push")}>
          {t("review:git_push_menu")}
        </MenuItem>
      </Menu>
      {step && status && (
        <CommitDialog
          sessionId={sessionId}
          status={status}
          initial={step}
          close={(byKey) => {
            setStep(undefined);
            // As the menus do: a key gives the focus back, the pointer does not.
            if (byKey) trigger.current?.focus();
          }}
        />
      )}
    </>
  );
}

function CommitDialog(props: {
  sessionId: string;
  status: GitStatusResult;
  initial: Step;
  close(byKey: boolean): void;
}) {
  const t = useT();
  const { client } = useHost();
  const query = useQueryClient();
  const { show } = useToasts();
  const title = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const pointer = useRef(false);
  const [step, setStep] = useState(props.initial);
  const [message, setMessage] = useState("");
  const [unstaged, setUnstaged] = useState(false);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<Failure>();
  const { status, sessionId } = props;
  const commits = step !== "push";
  const changed = status.repo_config_changed ?? [];

  useEffect(() => dialog.current?.showModal(), []);
  const close = () => {
    dialog.current?.close();
    props.close(!pointer.current);
  };

  const run = async () => {
    setBusy(true);
    setFailure(undefined);
    let staged = false;
    try {
      if (commits && unstaged) {
        // A fresh token: the host stages only from the unstaged diff as it is now.
        const diff = await client.request<GitDiffResult>("git/diff", {
          session_id: sessionId,
          scope: "unstaged",
        });
        if (diff.files.length > 0) {
          const target = { all: true };
          await client.command("git/stage", {
            session_id: sessionId,
            diff_token: diff.diff_token,
            target,
          });
          staged = true;
        }
      }
      if (commits) {
        await client.command("git/commit", { session_id: sessionId, message });
        staged = false;
      }
      if (step !== "commit") {
        // Without an upstream, a plain push fails (T123): set it.
        const upstream = status.upstream ? {} : { set_upstream: true };
        await client.command("git/push", { session_id: sessionId, ...upstream });
      }
      show({ id: "review", text: t(`review:git_done_${step}`) });
      close();
    } catch (error) {
      setFailure(failureOf(error, staged));
    } finally {
      setBusy(false);
      void query.invalidateQueries({ queryKey: ["session", sessionId] });
    }
  };

  return (
    <dialog
      ref={dialog}
      aria-labelledby={title}
      onPointerDown={() => (pointer.current = true)}
      onKeyDown={() => (pointer.current = false)}
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) close();
      }}
      className="m-auto w-105 rounded-2xl bg-surface-elevated p-5 text-sm text-fg shadow-menu backdrop:bg-backdrop"
    >
      <form
        className="flex flex-col gap-3 [&_[role]]:max-w-none"
        onSubmit={(event) => {
          event.preventDefault();
          void run();
        }}
      >
        <div>
          <h2 id={title} className="text-base font-medium">
            {t("review:git_title")}
          </h2>
          <p className="text-fg-secondary">
            {status.branch
              ? t("review:git_commit_to", { branch: status.branch })
              : t("review:git_detached")}
          </p>
        </div>
        {changed.length > 0 && (
          <Alert>{t("review:git_config_changed", { paths: changed.join("、") })}</Alert>
        )}
        <textarea
          aria-label={t("review:git_message")}
          placeholder={t("review:git_message")}
          value={message}
          disabled={!commits}
          autoFocus
          onChange={(event) => setMessage(event.target.value)}
          className="h-20 resize-none rounded-lg border border-border bg-surface px-2.5 py-2 outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
        />
        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={unstaged}
            disabled={!commits}
            onChange={(event) => setUnstaged(event.target.checked)}
          />
          {t("review:git_include_unstaged")}
        </label>
        <fieldset className="flex flex-col gap-1.5">
          <legend className="mb-1.5 text-fg-secondary">{t("review:git_next")}</legend>
          {STEPS.map((option) => (
            <label key={option} className="flex items-center gap-2">
              <input
                type="radio"
                name="next"
                checked={step === option}
                disabled={option !== "commit" && status.branch === undefined}
                onChange={() => setStep(option)}
              />
              {t(`review:git_step_${option}`)}
            </label>
          ))}
        </fieldset>
        {step !== "commit" && !status.upstream && status.branch && (
          <p className="text-fg-tertiary">{t("review:git_set_upstream")}</p>
        )}
        {failure && (
          <Alert tone="danger">
            {t(failure.key, { code: failure.code })}
            {failure.staged && ` ${t("review:git_left_staged")}`}
            {failure.stderr && (
              <details className="mt-1">
                <summary className="cursor-default text-fg-secondary">
                  {t("review:git_output")}
                </summary>
                <pre className="mt-1 max-h-40 overflow-auto font-mono text-xs whitespace-pre-wrap">
                  {failure.stderr}
                </pre>
              </details>
            )}
          </Alert>
        )}
        <div className="flex items-center gap-2">
          <span className="mr-auto text-fg-tertiary">{t("review:git_runs_hooks")}</span>
          <button
            type="button"
            disabled={busy}
            onClick={close}
            className="h-8 rounded-full bg-btn-secondary px-3.5 outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring"
          >
            {t("review:cancel")}
          </button>
          <button
            type="submit"
            disabled={busy || (commits && message.trim() === "")}
            className="h-8 rounded-full bg-btn-primary px-3.5 text-btn-primary-fg outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
          >
            {t("review:git_continue")}
          </button>
        </div>
      </form>
    </dialog>
  );
}
