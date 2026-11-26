import type { ContextItem } from "../../lib/deskClient";
import { BRIEF_PROMPT } from "../../lib/project-overview/documents";
import { currentManuscript } from "../../lib/project-overview/manuscript";
import type { SurfaceApi } from "./shared";
import ProjectNotes from "./ProjectNotes";

/** The brief is a view of the same editable notes, never a second record. */
export default function OverviewBrief(
  props: SurfaceApi & {
    onAsk: (prompt: string, context?: ContextItem[]) => void;
    selectedNoteId?: string;
  },
) {
  const manuscript = currentManuscript(props.data).paper;
  return (
    <section className="overview-brief-content" aria-label="Project context">
      <ProjectNotes {...props} compact context />
      {!props.data.notes.length && manuscript?.revision && (
        <button
          type="button"
          className="ui-button"
          onClick={() =>
            props.onAsk(BRIEF_PROMPT, [
              {
                role: "main",
                object: {
                  kind: "paper",
                  id: manuscript.revision!.id,
                  revision: manuscript.revision!.contentHash,
                },
              },
            ])
          }
        >
          Draft a brief from the paper
        </button>
      )}
    </section>
  );
}
