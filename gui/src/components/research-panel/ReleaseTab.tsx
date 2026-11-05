import WorkspaceReleasePanel from "../WorkspaceReleasePanel";
import { compactHash } from "./format";
import type { useResearchPanelController } from "./useResearchPanelController";
import type {
  ConversationSnapshot,
  ReviewHandoff,
} from "../../lib/workbenchTypes";

type Panel = ReturnType<typeof useResearchPanelController>;
type Props = Pick<
  Panel,
  | "workspaceId"
  | "busy"
  | "act"
  | "papers"
  | "selectedPaper"
  | "setProjectPaperId"
> & {
  snapshot: ConversationSnapshot | null;
  releaseView: "all" | "share" | "review";
  onError: (message: string) => void;
  onReviewHandoff?: (handoff: ReviewHandoff) => void;
};

export default function ReleaseTab({
  workspaceId,
  busy,
  act,
  papers,
  selectedPaper,
  setProjectPaperId,
  snapshot,
  releaseView,
  onError,
  onReviewHandoff,
}: Props) {
  return (
    <>
      {(releaseView === "all" || releaseView === "review") && (
        <label className="mb-4 block text-xs">
          Paper to review
          <select
            aria-label="Paper to review"
            className="mt-2 block w-full rounded border bg-transparent p-2"
            value={selectedPaper?.paper.id ?? ""}
            disabled={busy}
            onChange={(event) => setProjectPaperId(event.target.value)}
          >
            <option value="">Choose a paper…</option>
            {papers.map((item) => (
              <option key={item.paper.id} value={item.paper.id}>
                {item.paper.title}
              </option>
            ))}
          </select>
          {selectedPaper?.revision && (
            <span className="mt-2 block text-gray-500">
              Saved revision · {compactHash(selectedPaper.revision.contentHash)}
            </span>
          )}
        </label>
      )}
      <WorkspaceReleasePanel
        snapshot={snapshot}
        workspaceId={workspaceId}
        view={releaseView}
        selectedPaper={selectedPaper}
        busy={busy}
        onAction={(action) => {
          void act(action);
        }}
        onError={onError}
        onReviewHandoff={onReviewHandoff}
      />
    </>
  );
}
