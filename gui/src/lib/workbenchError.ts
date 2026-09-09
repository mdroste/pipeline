interface StructuredWorkbenchError {
  message?: unknown;
  recovery?: unknown;
}

function nonEmptyString(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

/**
 * Tauri preserves serializable Rust command errors as objects. Coercing one
 * with String(error) hides its useful message behind "[object Object]".
 */
export function workbenchErrorMessage(cause: unknown): string {
  if (cause instanceof Error) {
    return (
      nonEmptyString(cause.message) ?? "An unexpected project error occurred."
    );
  }

  const direct = nonEmptyString(cause);
  if (direct) return direct;

  if (cause && typeof cause === "object") {
    const structured = cause as StructuredWorkbenchError;
    const message = nonEmptyString(structured.message);
    const recovery = nonEmptyString(structured.recovery);
    if (message && recovery && recovery !== message)
      return `${message} ${recovery}`;
    if (message) return message;
    if (recovery) return recovery;
  }

  return "An unexpected project error occurred.";
}
