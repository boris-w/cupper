// The settings page (design 5.7.4, UI-2): the window to itself, the
// navigation in the sidebar's place with "返回应用" on top, and the section
// on the right, at most 768 wide.

import type { ComponentType } from "react";

import { useT } from "../app/providers";
import { useRouter } from "../app/router";
import { BackIcon } from "../components/icons";
import { ROW } from "../sidebar/parts";
import type { Route } from "../state/ui";
import { AgentsSection } from "./AgentsSection";
import { ArchivedSection } from "./ArchivedSection";
import { CliSection } from "./CliSection";
import { ConnectionsSection } from "./ConnectionsSection";

interface Entry {
  id: string;
  /** `focus` names an item of the section to bring into view. */
  Body: ComponentType<{ focus?: string }>;
}

/** The sections in the navigation's order; a page is one more line here (T147, T248). */
const SECTIONS: readonly [Entry, ...Entry[]] = [
  { id: "archived", Body: ArchivedSection },
  { id: "agents", Body: AgentsSection },
  { id: "connections", Body: ConnectionsSection },
  { id: "cli", Body: CliSection },
];

export function SettingsPage({ route }: { route: Extract<Route, { page: "settings" }> }) {
  const t = useT();
  const { navigate } = useRouter();
  const section = SECTIONS.find(({ id }) => id === route.section) ?? SECTIONS[0];
  const { Body } = section;
  return (
    <div className="flex h-full">
      <nav
        aria-label={t("settings:title")}
        className="flex w-(--layout-sidebar-w) shrink-0 flex-col bg-surface-sidebar px-2 text-base"
      >
        <div className="app-drag h-(--layout-titlebar-h) shrink-0" />
        <button
          type="button"
          onClick={() => navigate(route.from)}
          className={`${ROW} mb-2 text-fg-secondary hover:bg-hover`}
        >
          <BackIcon />
          {t("settings:back")}
        </button>
        {SECTIONS.map(({ id }) => (
          <button
            key={id}
            type="button"
            aria-current={id === section.id ? "page" : undefined}
            onClick={() => navigate({ ...route, section: id, focus: undefined })}
            className={`${ROW} hover:bg-hover aria-[current=page]:bg-hover`}
          >
            {t(`settings:section_${id}`)}
          </button>
        ))}
      </nav>
      <div className="min-w-0 flex-1 overflow-y-auto">
        <div className="app-drag h-(--layout-titlebar-h)" />
        <div className="mx-auto flex w-full max-w-192 min-w-[min(100%,320px)] flex-col gap-10 px-4 pb-10">
          <h1 className="text-heading-sm font-medium">{t(`settings:section_${section.id}`)}</h1>
          <Body focus={route.focus} />
        </div>
      </div>
    </div>
  );
}
