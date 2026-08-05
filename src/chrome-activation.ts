export interface ActiveChromeTarget {
  index: number;
  url: string;
}

export type ActiveChromeTargets = Map<string, ActiveChromeTarget>;

export function activateChromeTarget(
  activeTargets: ActiveChromeTargets,
  taskId: string,
  targetIndex: number,
  url: string,
): void {
  activeTargets.set(taskId, { index: targetIndex, url });
}

export function isChromeTargetActive(
  activeTargets: ActiveChromeTargets,
  taskId: string,
  targetIndex: number,
  url: string,
): boolean {
  const active = activeTargets.get(taskId);
  return active?.index === targetIndex && active.url === url;
}
