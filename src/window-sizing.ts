export const DEFAULT_BAR_HEIGHT = 560;
export const MIN_BAR_HEIGHT = 320;
export const SCREEN_VERTICAL_MARGIN = 24;

export function barHeightLimits(workAreaHeight: number): { min: number; max: number } {
  const max = Math.max(180, Math.floor(workAreaHeight - SCREEN_VERTICAL_MARGIN));
  return { min: Math.min(MIN_BAR_HEIGHT, max), max };
}

export function barHeightForContent(contentHeight: number, workAreaHeight: number): number {
  const limits = barHeightLimits(workAreaHeight);
  const preferred = Math.max(Math.min(DEFAULT_BAR_HEIGHT, limits.max), Math.ceil(contentHeight));
  return Math.min(limits.max, Math.max(limits.min, preferred));
}
