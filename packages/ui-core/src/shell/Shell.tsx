// The three columns of design 5.7.4: the sidebar, the session area under its
// title bar, and the side panel. The widths come from `computeLayout`; they
// reach the DOM as inline `width`, never as CSS custom properties. The
// settings page has the window to itself, its navigation in the sidebar's place.

import type { SessionGetResult } from "@cupper/protocol";
import { commandHandler } from "@cupper/shell-api";
import {
  use,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
  type RefObject,
} from "react";

import { SlotsContext } from "../app/slots";
import { useHost, useShell, useT } from "../app/providers";
import { useRouter } from "../app/router";
import { ChevronDownIcon, MoreIcon, PaneIcon } from "../components/icons";
import { Menu, MenuItem } from "../components/Menu";
import { ToolbarButton } from "../components/ToolbarButton";
import { ConnectionToast } from "../feedback/ConnectionToast";
import { useToasts } from "../feedback/toaster";
import { SidebarFooter, SidebarHeader, SidebarList } from "../sidebar";
import { computeLayout, ratioOf, type Layout, type Viewport } from "../layout/layout";
import { ReviewPanel } from "../review/ReviewPanel";
import { createReviewStore, OpenReview, ReviewContext, type ReviewTarget } from "../review/state";
import {
  failureText,
  SessionActionsProvider,
  SessionItems,
  useSessionActions,
  type OpenTarget,
} from "../session/actions";
import type { LayoutStore } from "../layout/store";
import { useStreams } from "../state/provider";
import { useEntry } from "../state/stream";
import { SessionTitle } from "./SessionTitle";
import { SessionShortcuts } from "./shortcuts";
import { Splitter } from "./Splitter";

function useViewport(): Viewport {
  const read = () => ({ width: window.innerWidth, height: window.innerHeight });
  const [view, setView] = useState(read);
  useEffect(() => {
    const onResize = () => setView(read());
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return view;
}

export function Shell({ store, children }: { store: LayoutStore; children: ReactNode }) {
  // The review panel's state, shared with the stream's change cards (T132).
  const [review] = useState(createReviewStore);
  const openReview = useCallback(
    (target: ReviewTarget) => {
      review.getState().show(target);
      store.set("sidePanelOpen", true);
    },
    [review, store],
  );
  return (
    <ReviewContext value={review}>
      <OpenReview value={openReview}>
        <SessionActionsProvider>
          <SessionShortcuts />
          <Frame store={store}>{children}</Frame>
        </SessionActionsProvider>
      </OpenReview>
    </ReviewContext>
  );
}

function Frame({ store, children }: { store: LayoutStore; children: ReactNode }) {
  const { api } = useShell();
  const { state } = useHost();
  const t = useT();
  const view = useViewport();
  const { prefs, overlayOpen } = useSyncExternalStore(store.subscribe, store.getState);
  const layout = computeLayout(view, prefs, overlayOpen);
  const narrow = layout.sidebar.mode === "overlay";
  const app = useRouter().route.page !== "settings";

  useEffect(() => {
    if (!narrow) {
      store.setOverlay(false);
      return;
    }
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && store.setOverlay(false);
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [narrow, store]);

  useEffect(
    () =>
      api.onCommand(
        commandHandler({
          toggle_sidebar: () => store.toggleSidebar(narrow),
          toggle_side_panel: () => store.toggleSidePanel(),
          open_review_tab: () => store.set("sidePanelOpen", true),
          toggle_file_tree: () => store.toggleFileTree(),
        }),
      ),
    [api, narrow, store],
  );

  // The side panel stays once it opened, or once the window was first idle:
  // hidden, not unmounted, so that the review document and its Worker are
  // ready when it opens (design 5.7.5, point 1).
  const shown = layout.sidePanel.visible && app;
  const [resident, setResident] = useState(shown);
  if (shown && !resident) setResident(true);
  useEffect(() => {
    const idle = requestIdleCallback(() => setResident(true));
    return () => cancelIdleCallback(idle);
  }, []);
  const panel = useRef<HTMLElement>(null);
  const closing = useSpring(panel, shown, layout.sidePanel.width);

  const sidebar = layout.sidebar.visible && app;
  const docked = sidebar && !narrow;
  return (
    <div className="flex h-full flex-col">
      <ConnectionToast state={state} />
      {sidebar && narrow && (
        <div className="fixed inset-0 z-10 bg-scrim" onClick={() => store.setOverlay(false)} />
      )}
      <TitleBar
        hidden={!app}
        sidebarWidth={docked ? layout.sidebar.width : undefined}
        sidebarOpen={layout.sidebar.visible}
        onSidebar={() => store.toggleSidebar(narrow)}
        sidePanelOpen={prefs.sidePanelOpen}
        onSidePanel={() => store.toggleSidePanel()}
      />
      {/* Below the title bar, the sidebar and the session area make one block
          with rounded corners on the window's material (Codex's shell, T148). */}
      <div className="flex min-h-0 flex-1">
        {sidebar && <Sidebar layout={layout} store={store} />}
        <div
          className={`flex min-w-0 flex-1 overflow-hidden ${app ? "rounded-r-(--radius-panel) bg-surface" : ""} ${app && !docked ? "rounded-l-(--radius-panel)" : ""}`}
        >
          <main className="min-h-0 min-w-0 flex-1 overflow-y-auto">{children}</main>
          {resident && (
            <aside
              ref={panel}
              hidden={!shown && !closing}
              aria-label={t("shell:side_panel")}
              style={{ width: layout.sidePanel.width }}
              className="relative flex shrink-0 flex-col border-l border-border"
            >
              <div
                role="tablist"
                className="flex h-(--layout-toolbar-h) shrink-0 items-center gap-1 pl-2"
              >
                <button
                  type="button"
                  role="tab"
                  aria-selected
                  className="h-8 min-w-22.5 rounded-lg border border-border bg-surface-elevated pr-1 pl-2.5 text-left text-sm"
                >
                  {t("shell:tab_changes")}
                </button>
              </div>
              <div role="tabpanel" className="min-h-0 flex-1">
                <ReviewPanel
                  fileTree={prefs.fileTreeOpen}
                  onFileTree={() => store.toggleFileTree()}
                  active={shown}
                />
              </div>
              <Splitter
                label={t("shell:resize_side_panel")}
                edge="left"
                width={layout.sidePanel.width}
                bounds={layout.sidePanel}
                onResize={(width, done) =>
                  store.set("sidePanelRatio", ratioOf(width, layout.sidePanel), done)
                }
              />
            </aside>
          )}
        </div>
      </div>
    </div>
  );
}

/** Codex's spring for the side panel (T084): 0.5 s, bounce 0.1, sampled for `linear()`. */
const SPRING =
  "linear(0, 0.1, 0.296, 0.494, 0.659, 0.783, 0.868, 0.925, 0.96, 0.98, 0.992, 0.998, 1)";

/**
 * Opens and closes `panel` on a spring, its width from 0 to `width` and
 * back; the session area beside it follows. Whether it is still closing,
 * and so stays shown until the spring ends. No spring with reduced motion.
 */
function useSpring(panel: RefObject<HTMLElement | null>, open: boolean, width: number): boolean {
  const [closing, setClosing] = useState(false);
  const was = useRef(open);
  useLayoutEffect(() => {
    const element = panel.current;
    if (was.current === open || !element?.animate) {
      was.current = open;
      return undefined;
    }
    was.current = open;
    const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
    const frames = [{ width: "0px" }, { width: `${width}px` }];
    const spring = element.animate(open ? frames : frames.toReversed(), {
      duration: still ? 0 : 500,
      easing: SPRING,
    });
    setClosing(!open);
    spring.onfinish = () => setClosing(false);
    return () => spring.cancel();
    // Only a change of `open` springs; a new width while dragged does not.
  }, [open, panel]);
  return closing;
}

const TARGETS: OpenTarget[] = ["terminal", "editor", "finder"];

/**
 * The 44 high title bar; `inset` leaves room for the traffic lights and the
 * sidebar toggle. "打开" and "⋯" act on the session shown, and only show then.
 */
export function TitleBar(props: {
  hidden: boolean;
  /** The docked sidebar's width: the bar's start, over it, holds the traffic lights and the toggle. */
  sidebarWidth?: number;
  sidebarOpen: boolean;
  onSidebar: () => void;
  sidePanelOpen: boolean;
  onSidePanel: () => void;
}) {
  const t = useT();
  const { client, handshake } = useHost();
  const { show } = useToasts();
  const actions = useSessionActions();
  const { route } = useRouter();
  const index = useStreams().index.view;
  useEntry(index, "view");
  const summary = route.page === "session" ? index.toJSON().sessions[route.sessionId] : undefined;
  const session = route.page === "session" ? route.sessionId : undefined;
  const copyPath = (id: string) =>
    client
      .request<SessionGetResult>("session/get", { session_id: id })
      .then(({ workspace }) => navigator.clipboard.writeText(workspace.path))
      .then(
        () => show({ id: "session", text: t("shell:path_copied") }),
        (error: unknown) => show({ id: "session", text: failureText(t, error) }),
      );
  return (
    <header
      hidden={props.hidden}
      className="app-drag flex h-(--layout-titlebar-h) shrink-0 items-center gap-1 pr-2"
    >
      <div
        style={{ width: props.sidebarWidth }}
        className="flex h-full shrink-0 items-center pl-(--layout-traffic-lights-w)"
      >
        <ToolbarButton
          label={t("shell:toggle_sidebar")}
          shortcut="⌘B"
          pressed={props.sidebarOpen}
          onClick={props.onSidebar}
        >
          <PaneIcon side="left" />
        </ToolbarButton>
        {props.sidebarWidth !== undefined && <span className="ml-auto h-4 w-px bg-border" />}
      </div>
      <div className="min-w-0 flex-1 pl-3">
        <SessionTitle summary={summary} />
      </div>
      {handshake?.host.weak_auth && (
        <ToolbarButton label={t("shell:weak_auth_detail")}>
          <span className="text-warning">{t("shell:weak_auth")}</span>
        </ToolbarButton>
      )}
      {session && (
        <Menu
          trigger={
            <ToolbarButton label={t("shell:open")}>
              {t("shell:open")}
              <ChevronDownIcon />
            </ToolbarButton>
          }
        >
          {TARGETS.map((target) => (
            <MenuItem key={target} onSelect={() => actions.open(session, target)}>
              {t(`shell:open_${target}`)}
            </MenuItem>
          ))}
        </Menu>
      )}
      {session && (
        <Menu
          trigger={
            <ToolbarButton label={t("shell:session_menu")}>
              <MoreIcon />
            </ToolbarButton>
          }
        >
          <SessionItems item={MenuItem} sessionId={session} />
          <MenuItem onSelect={() => void copyPath(session)}>{t("shell:copy_path")}</MenuItem>
        </Menu>
      )}
      <ToolbarButton
        label={t("shell:toggle_side_panel")}
        shortcut="⌥⌘B"
        pressed={props.sidePanelOpen}
        onClick={props.onSidePanel}
      >
        <PaneIcon side="right" />
      </ToolbarButton>
    </header>
  );
}

function Sidebar({ layout, store }: { layout: Layout; store: LayoutStore }) {
  const t = useT();
  const slots = use(SlotsContext);
  const overlay = layout.sidebar.mode === "overlay";
  return (
    <aside
      aria-label={t("shell:sidebar")}
      style={{ width: layout.sidebar.width }}
      className={`flex shrink-0 flex-col ${overlay ? "app-no-drag fixed bottom-0 left-0 top-(--layout-titlebar-h) z-20 rounded-r-(--radius-panel) bg-surface shadow-card" : "relative rounded-l-(--radius-panel) bg-(--sidebar-surface)"}`}
    >
      <div className="flex h-8 shrink-0 items-center px-4">
        {slots.sidebarHeader ?? <SidebarHeader />}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">{slots.sidebarList ?? <SidebarList />}</div>
      <div className="shrink-0">{slots.sidebarFooter ?? <SidebarFooter />}</div>
      <Splitter
        label={t("shell:resize_sidebar")}
        edge="right"
        width={layout.sidebar.width}
        bounds={layout.sidebar}
        onResize={(width, done) => store.set("sidebarWidth", width, done)}
      />
    </aside>
  );
}
