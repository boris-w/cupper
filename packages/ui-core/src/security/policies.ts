// The one place that creates Trusted Types policies (design 5.7.8). The main
// document's CSP requires them for scripts and allows exactly one,
// `cupper-worker` (`@cupper/shell-api`'s MAIN_DOCUMENT_CSP): it turns a
// same-origin script under `app://cupper/assets/`, a worker the build emits,
// into a script URL, and nothing else. There is no `default` policy, so no
// string ever becomes a script on its own; the lint rules keep it that way.

export const WORKER_POLICY = "cupper-worker";

/** What `new Worker` takes under Trusted Types. */
export interface ScriptUrlPolicy {
  createScriptURL(url: string): unknown;
}

/** `window.trustedTypes`, as far as used; the DOM library does not type it. */
export interface TrustedTypesFactory {
  createPolicy(name: string, rules: { createScriptURL(url: string): string }): ScriptUrlPolicy;
}

/** `url` when it is a script of the build's assets; throws otherwise. */
export function assetScript(url: string): string {
  const parsed = new URL(url);
  const asset =
    parsed.protocol === "app:" &&
    parsed.host === "cupper" &&
    parsed.pathname.startsWith("/assets/");
  if (!asset) {
    throw new TypeError(`not a script of app://cupper/assets/: ${url}`);
  }
  return parsed.href;
}

/** The worker policy of `factory`; without Trusted Types, the same check on plain strings. */
export function workerPolicy(factory: TrustedTypesFactory | undefined): ScriptUrlPolicy {
  return factory
    ? factory.createPolicy(WORKER_POLICY, { createScriptURL: assetScript })
    : { createScriptURL: assetScript };
}

let created: ScriptUrlPolicy | undefined;

/** A worker script's URL for `new Worker`, through the one policy, made on first use. */
export function workerScriptUrl(url: string): unknown {
  const factory = (globalThis as { trustedTypes?: TrustedTypesFactory }).trustedTypes;
  created ??= workerPolicy(factory);
  return created.createScriptURL(url);
}
