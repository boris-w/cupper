// The sidebar's default slots (T084): its parts on the index stream.

import { useHost, useT } from "../app/providers";
import { useEntry } from "../state/stream";
import { useStreams } from "../state/provider";
import { ElevationBanner, HostStatus, ProjectList } from "./parts";

export function SidebarHeader() {
  const t = useT();
  const { state } = useHost();
  const { view } = useStreams().index;
  useEntry(view, "host");
  return (
    <div className="flex items-center gap-2 text-base font-medium">
      {t("shell:product")}
      <HostStatus state={state} host={view.toJSON().host} />
    </div>
  );
}

export function SidebarList() {
  const { view } = useStreams().index;
  useEntry(view, "view");
  const { projects, sessions } = view.toJSON();
  return <ProjectList projects={Object.values(projects)} sessions={Object.values(sessions)} />;
}

export function SidebarFooter() {
  const { view } = useStreams().index;
  useEntry(view, "elevations");
  return <ElevationBanner elevations={Object.values(view.toJSON().elevations)} />;
}
