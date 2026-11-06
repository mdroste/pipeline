// Native exit does not reliably unload the webview. Editors synchronously
// retain their current drafts here before quit or an update can destroy it.
export const DRAFT_FLUSH_EVENT = "pipeline:flush-drafts";

export function flushDrafts() {
  if (!window.dispatchEvent(new Event(DRAFT_FLUSH_EVENT, { cancelable: true })))
    throw new Error(
      "Drafts could not be retained. Save your files before closing Pipeline.",
    );
}
