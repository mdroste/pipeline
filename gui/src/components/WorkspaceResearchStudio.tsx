import { lazy, Suspense } from "react";
import Jobs from "./research-studio/Jobs";
import type { StudioProps } from "./research-studio/shared";
const Manuscript = lazy(() => import("./research-studio/Manuscript"));
const Responses = lazy(() => import("./research-studio/Responses"));

/** The manuscript editor or the response workspace, with their build jobs. */
export default function WorkspaceResearchStudio(
  props: StudioProps & {
    active?: boolean;
    tool: "manuscript" | "responses";
  },
) {
  const { tool } = props;
  return (
    <div
      className={
        tool === "manuscript" ? "flex min-h-0 flex-1 flex-col" : "space-y-5"
      }
    >
      <Jobs
        visible={props.active}
        quiet={tool === "manuscript"}
        workspaceId={props.workspaceId}
        onCompleted={() => void props.onRefresh()}
      />
      <Suspense
        fallback={<p className="p-4 text-sm">Loading research tools…</p>}
      >
        {tool === "manuscript" ? (
          <Manuscript {...props} />
        ) : (
          <Responses {...props} />
        )}
      </Suspense>
    </div>
  );
}
