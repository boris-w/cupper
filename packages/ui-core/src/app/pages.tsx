import { createContext, use, type ReactNode } from "react";

import { ComposerArea } from "../composer/QueueList";
import { SessionBanners } from "../session/SessionBanners";
import { NewSessionPage } from "../newsession/NewSessionPage";
import { StreamList } from "../stream/StreamList";
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

/** A session: banners and the message stream above, the composer at the bottom. */
export function SessionPage({ sessionId }: { sessionId: string }) {
  const { sessionComposer } = use(SlotsContext);
  return (
    <div className="mx-auto flex h-full w-full max-w-(--layout-thread-max-w) flex-col px-(--layout-thread-gutter)">
      <SessionBanners sessionId={sessionId} />
      <StreamList sessionId={sessionId} />
      <div className="shrink-0 pb-(--layout-composer-bottom)">
        {sessionComposer ?? <ComposerArea sessionId={sessionId} />}
      </div>
    </div>
  );
}

/** The main area's page; each session gets fresh component state. */
export function Page() {
  const { route } = useRouter();
  return route.page === "session" ? (
    <SessionPage key={route.sessionId} sessionId={route.sessionId} />
  ) : (
    <NewSessionPage />
  );
}
