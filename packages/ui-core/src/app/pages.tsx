import { createContext, use, type ReactNode } from "react";

import { useRouter } from "./router";

/** The places later tasks fill: the sidebar's parts (T081) and the composers (T086, T083). */
export interface Slots {
  sidebarHeader?: ReactNode;
  sidebarList?: ReactNode;
  sidebarFooter?: ReactNode;
  newSessionComposer?: ReactNode;
  sessionComposer?: ReactNode;
}

export const SlotsContext = createContext<Slots>({});

/** The new session page: the composer's top sits at 42% of the window's height. */
export function NewSessionPage() {
  const { newSessionComposer } = use(SlotsContext);
  return (
    <div className="mx-auto w-full max-w-(--layout-thread-max-w) px-(--layout-thread-gutter) pt-[calc(var(--layout-home-composer-anchor)-var(--layout-titlebar-h))]">
      {newSessionComposer}
    </div>
  );
}

/** A session: banners and the message stream above, the composer at the bottom. */
export function SessionPage() {
  const { sessionComposer } = use(SlotsContext);
  return (
    <div className="mx-auto flex h-full w-full max-w-(--layout-thread-max-w) flex-col px-(--layout-thread-gutter)">
      <div className="min-h-0 flex-1" />
      <div className="shrink-0 pb-(--layout-composer-bottom)">{sessionComposer}</div>
    </div>
  );
}

/** The main area's page; each session gets fresh component state. */
export function Page() {
  const { route } = useRouter();
  return route.page === "session" ? <SessionPage key={route.sessionId} /> : <NewSessionPage />;
}
