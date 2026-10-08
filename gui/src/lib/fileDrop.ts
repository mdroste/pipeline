// Files dragged from the desktop onto the window. The native shell reports
// real paths, which the browser's own drop event does not.

export type FileDropEvent =
  | { type: "over"; x: number; y: number }
  | { type: "drop"; x: number; y: number; paths: string[] }
  | { type: "leave" };

/** Subscribe to file drags over the window. Resolves to an unsubscribe. */
export async function onFileDrop(
  handler: (event: FileDropEvent) => void,
): Promise<() => void> {
  try {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    return await getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type === "leave") {
        handler({ type: "leave" });
        return;
      }
      // Positions arrive in physical pixels.
      const scale = window.devicePixelRatio || 1;
      const x = payload.position.x / scale;
      const y = payload.position.y / scale;
      if (payload.type === "drop")
        handler({ type: "drop", x, y, paths: payload.paths });
      else handler({ type: "over", x, y });
    });
  } catch {
    // Outside the desktop shell there are no native file drags.
    return () => undefined;
  }
}
