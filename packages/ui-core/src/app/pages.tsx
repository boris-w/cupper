import { use } from "react";

import { ComposerArea } from "../composer/QueueList";
import { SessionBanners } from "../session/SessionBanners";
import { NewSessionPage } from "../newsession/NewSessionPage";
import { SettingsPage } from "../settings/SettingsPage";
import { StreamList } from "../stream/StreamList";
import { useRouter } from "./router";
import { SlotsContext } from "./slots";

const COLUMN = "mx-auto w-full max-w-(--layout-thread-max-w) px-(--layout-thread-gutter)";

/**
 * A session: banners and the message stream above, the composer at the
 * bottom, each in the content column. The stream spans the whole area, so
 * that its scrollbar is at the area's edge, not on the column (T256).
 */
export function SessionPage({ sessionId }: { sessionId: string }) {
  const { sessionComposer } = use(SlotsContext);
  return (
    <div className="flex h-full w-full flex-col">
      <div className={COLUMN}>
        <SessionBanners sessionId={sessionId} />
      </div>
      <StreamList sessionId={sessionId} />
      <div className={`${COLUMN} shrink-0 pb-(--layout-composer-bottom)`}>
        {sessionComposer ?? <ComposerArea sessionId={sessionId} />}
      </div>
    </div>
  );
}

/** The main area's page; each session gets fresh component state. */
export function Page() {
  const { route } = useRouter();
  if (route.page === "settings") {
    return <SettingsPage route={route} />;
  }
  return route.page === "session" ? (
    <SessionPage key={route.sessionId} sessionId={route.sessionId} />
  ) : (
    <NewSessionPage />
  );
}
