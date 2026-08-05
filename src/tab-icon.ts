interface GraphemeSegment {
  segment: string;
}

interface GraphemeSegmenter {
  segment(value: string): Iterable<GraphemeSegment>;
}

type SegmenterConstructor = new (
  locale?: string,
  options?: { granularity: "grapheme" },
) => GraphemeSegmenter;

function graphemes(value: string): string[] {
  const Segmenter = (Intl as unknown as { Segmenter?: SegmenterConstructor }).Segmenter;
  if (!Segmenter) return Array.from(value);
  return Array.from(new Segmenter(undefined, { granularity: "grapheme" }).segment(value),
    ({ segment }) => segment);
}

function isLetterOrNumber(value: string): boolean {
  return /^[\p{L}\p{N}]$/u.test(value);
}

export function normalizeTabIcon(value: string): string {
  const parts = graphemes(value.trim());
  if (parts.length === 0) return "";
  if (!isLetterOrNumber(parts[0])) return parts[0];
  return parts.slice(0, 2).join("").toLocaleUpperCase();
}

function titleInitials(title: string): string {
  const words = title.trim().split(/[\s\-_]+/u).filter(Boolean);
  if (words.length > 1) {
    return words.slice(0, 2)
      .map((word) => graphemes(word)[0] || "")
      .join("")
      .toLocaleUpperCase();
  }
  const parts = graphemes(words[0] || "?");
  if (/^[A-Za-z0-9]$/.test(parts[0] || "")) {
    return parts.slice(0, 2).join("").toLocaleUpperCase();
  }
  return parts[0] || "?";
}

export function tabIconForTask(configured: string | undefined, title: string): string {
  return normalizeTabIcon(configured || "") || titleInitials(title);
}
