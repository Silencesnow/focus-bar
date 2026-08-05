import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type Point = { x: number; y: number };

export type HoverClassTarget = {
  classList: {
    add(value: string): void;
    remove(value: string): void;
  };
};

export type HoverHitTarget = {
  closest(selector: string): HoverClassTarget | null;
};

const HOVER_TARGET_SELECTORS = [
  "#settings-button",
  ".task-card",
  ".tool-button",
  "#suggestion.clickable",
  ".ctx-item",
];

const NATIVE_HOVER_CLASS = "is-native-hovered";

export function applyNativeHoverAtCursor(
  previous: HoverClassTarget[],
  cursor: Point,
  windowOrigin: Point,
  scaleFactor: number,
  elementFromPoint: (x: number, y: number) => HoverHitTarget | null,
): HoverClassTarget[] {
  const x = (cursor.x - windowOrigin.x) / scaleFactor;
  const y = (cursor.y - windowOrigin.y) / scaleFactor;
  const hit = elementFromPoint(x, y);
  const next: HoverClassTarget[] = [];

  if (hit) {
    for (const selector of HOVER_TARGET_SELECTORS) {
      const target = hit.closest(selector);
      if (target && !next.includes(target)) next.push(target);
    }
  }

  for (const target of previous) {
    if (!next.includes(target)) target.classList.remove(NATIVE_HOVER_CLASS);
  }
  for (const target of next) {
    target.classList.add(NATIVE_HOVER_CLASS);
  }
  return next;
}

export async function startInactiveHoverTracking(): Promise<() => void> {
  const win = getCurrentWindow();
  let windowOrigin: Point = await win.outerPosition();
  let scaleFactor = await win.scaleFactor();
  let hovered: HoverClassTarget[] = [];
  let lastCursor: Point | null = null;

  const syncHover = () => {
    if (!lastCursor) return;
    hovered = applyNativeHoverAtCursor(
      hovered,
      lastCursor,
      windowOrigin,
      scaleFactor,
      (x, y) => document.elementFromPoint(x, y),
    );
  };

  const unlistenMoved = await win.onMoved(({ payload }) => {
    windowOrigin = payload;
    syncHover();
  });
  const unlistenScale = await win.onScaleChanged(({ payload }) => {
    scaleFactor = payload.scaleFactor;
    syncHover();
  });
  const unlistenCursor = await listen<Point>("global-cursor-moved", ({ payload }) => {
    lastCursor = payload;
    syncHover();
  });

  return () => {
    unlistenMoved();
    unlistenScale();
    unlistenCursor();
    for (const target of hovered) target.classList.remove(NATIVE_HOVER_CLASS);
  };
}
