export const taskTime = (value: number | null) =>
  value === null
    ? "Waiting for input"
    : new Date(value * 1000).toLocaleString([], {
        month: "short",
        day: "numeric",
        hour: "numeric",
        minute: "2-digit",
      });
export function stateLabel(state: string) {
  return (
    (
      {
        attention: "Needs attention",
        draft: "Ready to start",
        cancelling: "Stopping",
        finished: "Finished",
        cancelled: "Stopped",
      } as Record<string, string>
    )[state] ?? state.charAt(0).toUpperCase() + state.slice(1)
  );
}
