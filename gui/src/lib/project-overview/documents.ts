import type { OverviewBlock } from "./types";
import type { ProjectHome } from "../projectClient";
import { block, overviewDay } from "./presentation";

// ---- Stage ----------------------------------------------------------------

const ROLE_LABELS: Record<string, string> = {
  manuscript: "Main paper",
  appendix: "Appendix",
};

/** Documents in this project: every imported paper with its role, once there is more than one. */
export function documentsBlock(data: ProjectHome): OverviewBlock | null {
  const papers = data.papers.filter((paper) => paper.revision);
  if (papers.length < 2) return null;
  return block(
    "documents",
    "Documents in this project",
    papers.map((paper) => ({
      id: `document-${paper.paper.id}`,
      text: paper.paper.title,
      detail: [
        ROLE_LABELS[paper.paper.role] ?? "Supporting document",
        `saved ${overviewDay(paper.revision!.capturedAt)}`,
      ].join(" · "),
      action: {
        kind: "document",
        label: "Open",
        revisionId: paper.revision!.id,
      },
    })),
  );
}

/** The prompt behind "Draft a brief from the manuscript". */
export const BRIEF_PROMPT =
  "Read the current version of the paper in this project and propose a short research brief as project notes for me to accept: the research question, the main contribution, and the key identifying assumptions (one note each, in my paper's own terms). Propose them as suggested notes; do not edit any files.";
