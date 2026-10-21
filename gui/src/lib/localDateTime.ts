/** datetime-local uses the target date's offset, which may differ from today. */
export function localDateTime(date: Date): string {
  if (!Number.isFinite(date.getTime())) return "";
  return new Date(date.getTime() - date.getTimezoneOffset() * 60000)
    .toISOString()
    .slice(0, 16);
}

/** Reject DST gaps; for repeated wall times Date selects the earlier occurrence. */
export function parseLocalDateTime(value: string): number | null {
  const date = new Date(value);
  return value && localDateTime(date) === value
    ? Math.floor(date.getTime() / 1000)
    : null;
}

export const localDateTimeHint =
  "Uses your local time zone. When clocks turn back, repeated times use the earlier occurrence.";

export function readLocalDateTime(input: HTMLInputElement): number | null {
  const at = parseLocalDateTime(input.value);
  input.setCustomValidity(
    at === null && input.value
      ? "This local time does not exist. Choose a time outside the daylight saving clock change."
      : "",
  );
  if (at === null && input.value) input.reportValidity();
  return at;
}
