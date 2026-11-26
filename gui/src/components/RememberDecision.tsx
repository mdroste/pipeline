import { useState } from "react";
import { researchClient } from "../lib/workbench/researchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import { appEvents } from "../lib/appEvents";

/** Deliberately curated memory: the transcript stays intact and acceptance is separate. */
export default function RememberDecision({
  workspaceId,
  conversationId,
  messageId,
  text,
}: {
  workspaceId: string;
  conversationId: string;
  messageId: string;
  text: string;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState("");
  return (
    <div className="my-2 text-xs">
      {draft === null ? (
        <button
          type="button"
          className="workspace-copy-button"
          onClick={() => {
            setDraft(text);
            setStatus("");
          }}
        >
          Remember this decision
        </button>
      ) : (
        <form
          className="space-y-2"
          onSubmit={(event) => {
            event.preventDefault();
            if (!draft.trim() || saving) return;
            setSaving(true);
            setStatus("");
            void researchClient
              .createNote({
                workspaceId,
                paperId: null,
                kind: "decision",
                body: `${draft.trim()}\n\nSource: conversation ${conversationId}, message ${messageId}.`,
                state: "proposed",
                origin: "user",
                pinned: false,
                operationId: crypto.randomUUID(),
              })
              .then(() => {
                setDraft(null);
                setStatus("Saved to suggested project notes for your review.");
                appEvents.emit("project-notes-changed", { workspaceId });
              })
              .catch((error) => setStatus(workbenchErrorMessage(error)))
              .finally(() => setSaving(false));
          }}
        >
          <label>
            Decision to remember
            <textarea
              className="ui-input block w-full"
              rows={4}
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
              disabled={saving}
            />
          </label>
          <button className="ui-button" disabled={saving || !draft.trim()}>
            {saving ? "Saving…" : "Save proposed note"}
          </button>{" "}
          <button
            type="button"
            className="ui-button"
            disabled={saving}
            onClick={() => setDraft(null)}
          >
            Cancel
          </button>
        </form>
      )}
      {status && <p role="status">{status}</p>}
    </div>
  );
}
