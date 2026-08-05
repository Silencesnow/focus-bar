export const MIN_NOTE_HEIGHT = 58;
export const MAX_NOTE_HEIGHT = 280;

export function noteHeightFromDrag(startHeight: number, deltaY: number): number {
  return Math.min(MAX_NOTE_HEIGHT, Math.max(MIN_NOTE_HEIGHT, Math.round(startHeight + deltaY)));
}
