// The roots of the component tree (design 5.7.5): the shell, the host
// connection, the text and the theme. The state layer (T080) builds on these.

import { connectShell, HostClient, type ClientState } from "@cupper/host-client";
import { createI18n } from "@cupper/i18n";
import type { Handshake, ShellAPI, ShellInfo, Theme } from "@cupper/shell-api";
import {
  createContext,
  use,
  useEffect,
  useLayoutEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

/** A context's value, or an error that names the missing provider. */
export function required<T>(value: T | null, name: string): T {
  if (value === null) {
    throw new Error(`${name} is missing above this component`);
  }
  return value;
}

export interface ShellValue {
  api: ShellAPI;
  info: ShellInfo;
}

const ShellContext = createContext<ShellValue | null>(null);

export function ShellProvider({ children, ...value }: ShellValue & { children: ReactNode }) {
  const { api, info } = value;
  return (
    <ShellContext value={useMemo(() => ({ api, info }), [api, info])}>{children}</ShellContext>
  );
}

export const useShell = () => required(use(ShellContext), "ShellProvider");

export interface HostValue {
  client: HostClient;
  state: ClientState;
  handshake: Handshake | undefined;
}

const HostContext = createContext<HostValue | null>(null);

const VALIDATION = import.meta.env.PROD ? "non_delta" : "all";

/** Opens this window's data connection once and follows its state. */
export function HostProvider({ children }: { children: ReactNode }) {
  const { api } = useShell();
  // Production builds check every event but `item.delta` (design 5.7.6).
  const [client] = useState(() => new HostClient({ validation: VALIDATION }));
  const [value, setValue] = useState<HostValue>({
    client,
    state: client.state,
    handshake: undefined,
  });
  useEffect(() => {
    const offState = client.onState((state) => {
      setValue({ client, state, handshake: client.handshake });
    });
    let offShell: (() => void) | undefined;
    let closed = false;
    connectShell(api.host, client).then(
      (off) => (closed ? off() : (offShell = off)),
      (err: unknown) => {
        console.warn("host: cannot open the connection", err);
        // The main process's code for a host it cannot reach.
        client.fail("host_unreachable");
      },
    );
    return () => {
      closed = true;
      offState();
      offShell?.();
      client.close();
    };
  }, [api, client]);
  return <HostContext value={value}>{children}</HostContext>;
}

export const useHost = () => required(use(HostContext), "HostProvider");

/** A text by `namespace:key`; values are interpolated as plain text. */
export type Translate = (key: string, values?: Record<string, unknown>) => string;

const I18nContext = createContext<Translate | null>(null);

export function I18nProvider({ locale, children }: { locale: string; children: ReactNode }) {
  const t = useMemo<Translate>(() => {
    const i18n = createI18n(locale);
    return (key, values) => i18n.t(key, values);
  }, [locale]);
  return <I18nContext value={t}>{children}</I18nContext>;
}

export const useT = () => required(use(I18nContext), "I18nProvider");

/**
 * The theme the shell reports: the system's, unless the settings pin one.
 * `data-theme` on the root sets the tokens' `color-scheme`.
 */
export function ThemeProvider({ children }: { children: ReactNode }) {
  const { api, info } = useShell();
  useLayoutEffect(() => {
    const apply = (theme: Theme) => (document.documentElement.dataset.theme = theme);
    apply(info.theme);
    return api.system.onThemeChange(apply);
  }, [api, info]);
  return children;
}
