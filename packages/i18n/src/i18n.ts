// UI text keyed by semantic codes (design 5.7.7). The host sends codes, never
// text; each feature keeps its text in `locales/<locale>/<feature>.json`, one
// i18next namespace per file. M1a ships one language; M1b adds English.

import { createInstance, type i18n } from "i18next";

import command from "./locales/zh-CN/command.json";
import host from "./locales/zh-CN/host.json";
import privileged from "./locales/zh-CN/privileged.json";
import shell from "./locales/zh-CN/shell.json";
import stream from "./locales/zh-CN/stream.json";
import update from "./locales/zh-CN/update.json";

export const DEFAULT_LOCALE = "zh-CN";

const resources = {
  "zh-CN": { command, host, privileged, shell, stream, update },
};

export type Namespace = keyof (typeof resources)["zh-CN"];

export const NAMESPACES = Object.keys(resources[DEFAULT_LOCALE]) as Namespace[];

/**
 * An i18next instance for `locale`, ready on return: the text is bundled, so
 * nothing loads. A locale without text falls back to the default one.
 */
export function createI18n(locale: string = DEFAULT_LOCALE): i18n {
  const instance = createInstance();
  void instance.init({
    lng: locale,
    fallbackLng: DEFAULT_LOCALE,
    resources,
    ns: NAMESPACES,
    initAsync: false,
    // React and the native dialogs both take plain text; HTML escaping would
    // show up as `&lt;`. Agent text goes through `escapeInvisible` instead.
    interpolation: { escapeValue: false },
  });
  return instance;
}
