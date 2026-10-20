// The three columns of design 5.7.4 as pure functions: the window size and the
// layout preferences in, every width out. The components only draw the result.

export const SIDEBAR = { default: 288, min: 240, max: 520 } as const;
/** The main area keeps this much beside the docked sidebar. */
export const MAIN_MIN = 240;
export const SIDE_PANEL_MIN = 320;
/** The session area keeps this much beside the side panel. */
export const CHAT_MIN = 352;
/** Below this window width, the sidebar floats over the content. */
export const NARROW = 768;
/** One arrow key on a resize handle. */
export const KEY_STEP = 16;

/** The layout preferences, kept by the shell (`ShellAPI.prefs`). */
export interface LayoutPrefs {
  sidebarOpen: boolean;
  sidebarWidth: number;
  /** What the user asked for; the panel still hides while it does not fit. */
  sidePanelOpen: boolean;
  /** Where the width sits between its minimum (0) and maximum (1); `null`: the default width. */
  sidePanelRatio: number | null;
}

export const DEFAULT_PREFS: LayoutPrefs = {
  sidebarOpen: true,
  sidebarWidth: SIDEBAR.default,
  sidePanelOpen: false,
  sidePanelRatio: null,
};

export interface Viewport {
  width: number;
  height: number;
}

export interface Bounds {
  min: number;
  max: number;
}

export interface Layout {
  sidebar: Bounds & {
    /** `overlay`: floats over the content, as in a window narrower than 768. */
    mode: "docked" | "overlay";
    visible: boolean;
    width: number;
  };
  /** The session area and the side panel together. */
  main: number;
  sidePanel: Bounds & {
    visible: boolean;
    /** Open, but hidden because the session area would get less than its minimum. */
    collapsed: boolean;
    width: number;
  };
}

export const clamp = (value: number, { min, max }: Bounds) => Math.min(max, Math.max(min, value));

/** The sidebar stays 240–520 wide and leaves the main area 240. */
export function sidebarBounds(windowWidth: number): Bounds {
  return {
    min: SIDEBAR.min,
    max: Math.max(SIDEBAR.min, Math.min(SIDEBAR.max, windowWidth - MAIN_MIN)),
  };
}

/** The side panel is at least 320 wide and leaves the session area 352. */
export function sidePanelBounds(main: number): Bounds {
  return { min: SIDE_PANEL_MIN, max: Math.max(SIDE_PANEL_MIN, main - CHAT_MIN) };
}

/** The width before the user drags: max(320, min(height × 1.6, main − 500), min(640, main − 352)). */
export function defaultSidePanelWidth(view: Viewport, main: number): number {
  const width = Math.max(
    SIDE_PANEL_MIN,
    Math.min(view.height * 1.6, main - 500),
    Math.min(640, main - CHAT_MIN),
  );
  return clamp(width, sidePanelBounds(main));
}

export function ratioOf(width: number, { min, max }: Bounds): number {
  return max > min ? (clamp(width, { min, max }) - min) / (max - min) : 0;
}

/**
 * The layout for a window. `overlayOpen` is the floating sidebar's state; it
 * only counts in a narrow window, where the docked one's preference does not.
 */
export function computeLayout(view: Viewport, prefs: LayoutPrefs, overlayOpen: boolean): Layout {
  const mode = view.width < NARROW ? "overlay" : "docked";
  const sideBounds = sidebarBounds(view.width);
  const sidebarVisible = mode === "overlay" ? overlayOpen : prefs.sidebarOpen;
  const sidebarWidth = clamp(prefs.sidebarWidth, sideBounds);
  const main = view.width - (mode === "docked" && sidebarVisible ? sidebarWidth : 0);

  const panelBounds = sidePanelBounds(main);
  const fits = main >= CHAT_MIN + SIDE_PANEL_MIN;
  const panelWidth =
    prefs.sidePanelRatio === null
      ? defaultSidePanelWidth(view, main)
      : panelBounds.min + prefs.sidePanelRatio * (panelBounds.max - panelBounds.min);
  return {
    sidebar: { ...sideBounds, mode, visible: sidebarVisible, width: sidebarWidth },
    main,
    sidePanel: {
      ...panelBounds,
      visible: prefs.sidePanelOpen && fits,
      collapsed: prefs.sidePanelOpen && !fits,
      width: Math.round(clamp(panelWidth, panelBounds)),
    },
  };
}

/**
 * A key on a resize handle: Home is the minimum, End the maximum, and the
 * arrow toward `grow` widens by one step. Other keys give `undefined`.
 */
export function keyResize(
  key: string,
  width: number,
  bounds: Bounds,
  grow: "ArrowLeft" | "ArrowRight",
): number | undefined {
  switch (key) {
    case "Home":
      return bounds.min;
    case "End":
      return bounds.max;
    case "ArrowLeft":
    case "ArrowRight":
      return clamp(width + (key === grow ? KEY_STEP : -KEY_STEP), bounds);
    default:
      return undefined;
  }
}

/** A drag from `startX` to `x` on a handle whose panel widens toward `grow`. */
export function dragResize(
  start: number,
  startX: number,
  x: number,
  bounds: Bounds,
  grow: "left" | "right",
): number {
  return clamp(start + (grow === "right" ? x - startX : startX - x), bounds);
}
