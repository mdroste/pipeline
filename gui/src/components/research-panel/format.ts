export function compactHash(value?: string | null) {
  return value ? `${value.slice(0, 10)}…${value.slice(-6)}` : "—";
}
