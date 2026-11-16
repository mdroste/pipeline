import { documentTypeLabel } from "../documentTypes";
import type { OverviewBlock } from "./types";
import type { ProjectHome } from "../projectClient";
import { block, overviewDay } from "./presentation";

/** Documents in this project: every imported paper with its role, once there is more than one. */
export function documentsBlock(data: ProjectHome): OverviewBlock | null {
  const papers = data.papers
    .filter((paper) => paper.revision && !paper.paper.archived)
    .sort(
      (a, b) =>
        Number(Boolean(b.paper.pinned)) - Number(Boolean(a.paper.pinned)) ||
        b.paper.updatedAt.localeCompare(a.paper.updatedAt),
    );
  if (!papers.length) return null;
  return block("documents", "Documents in this project", [
    ...papers.slice(0, 5).map((paper) => ({
      id: `document-${paper.paper.id}`,
      text: paper.paper.title,
      detail: [
        documentTypeLabel(paper.paper),
        `saved ${overviewDay(paper.revision!.capturedAt)}`,
      ].join(" · "),
      action: {
        kind: "document" as const,
        label: "Open",
        revisionId: paper.revision!.id,
      },
    })),
    ...(papers.length > 5
      ? [
          {
            id: "all-documents",
            text: `${papers.length} documents in this project`,
            action: {
              kind: "tab" as const,
              label: "Browse all documents",
              tab: "documents" as const,
            },
          },
        ]
      : []),
  ]);
}

/** The prompt behind "Draft a brief from the manuscript". */
export const BRIEF_PROMPT =
  "Read the current version of the paper in this project and propose a short research brief as project notes for me to accept: the research question, the main contribution, and the key identifying assumptions (one note each, in my paper's own terms). Propose them as suggested notes; do not edit any files.";
