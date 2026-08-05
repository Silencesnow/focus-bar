interface ScrollViewport {
  scrollTop: number;
}

interface Blurrable {
  blur(): void;
}

export function resetSettingsViewport(
  viewport: ScrollViewport,
  activeElement?: Blurrable | null,
): void {
  activeElement?.blur();
  viewport.scrollTop = 0;
}
