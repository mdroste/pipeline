export const AUTHOR_NOTE_MAX_CHARS = 4_000;

export function normalizeAuthorNote(
  value: string | null | undefined,
): string | null {
  return (
    value
      ?.replace(/\r\n?/g, "\n")
      .replace(/^\p{White_Space}+|\p{White_Space}+$/gu, "") || null
  );
}

export function authorNoteLength(value: string | null | undefined): number {
  return Array.from(normalizeAuthorNote(value) ?? "").length;
}
