import type { ProjectHome } from "../../lib/projectClient";
import type { Workspace } from "../../lib/workbench/sessionTypes";
import type { OverviewAction } from "../../lib/project-overview/types";
import { documentSourcePath } from "../../lib/project-overview/documentSource";
import { documentTypeLabel } from "../../lib/documentTypes";
import { projectDate } from "../../lib/projectIndex";
import { currentManuscript } from "../../lib/project-overview/manuscript";

export default function OverviewWorkingSet({
  data,
  workspace,
  onAction,
}: {
  data: ProjectHome;
  workspace: Workspace | null;
  onAction: (action: OverviewAction) => void;
}) {
  const primaryId = currentManuscript(data).paper?.paper.id;
  const documents = data.papers
    .filter((item) => item.revision && !item.paper.archived)
    .sort(
      (a, b) =>
        Number(b.paper.id === primaryId) - Number(a.paper.id === primaryId) ||
        Number(Boolean(b.paper.pinned)) - Number(Boolean(a.paper.pinned)) ||
        b.paper.updatedAt.localeCompare(a.paper.updatedAt),
    );
  return (
    <div className="overview-working-set">
      {documents.slice(0, 3).map((document) => {
        const revision = document.revision!;
        const source = documentSourcePath(
          document,
          workspace?.root,
          data.inventory?.body.files.map((file) => file.path) ?? [],
        );
        return (
          <article className="overview-artifact" key={document.paper.id}>
            <div>
              <h3 title={document.paper.title}>{document.paper.title}</h3>
              <p className="overview-meta">
                {documentTypeLabel(document.paper)} · Saved{" "}
                {projectDate(revision.capturedAt)}
              </p>
            </div>
            <div className="overview-actions">
              {source && (
                <button
                  type="button"
                  className="ui-button ui-button--link ui-button--sm"
                  aria-label={`Edit ${document.paper.title}`}
                  onClick={() =>
                    onAction({
                      kind: "document",
                      label: "Edit",
                      revisionId: revision.id,
                      mode: "edit",
                    })
                  }
                >
                  Edit
                </button>
              )}
              <button
                type="button"
                className="ui-button ui-button--link ui-button--sm"
                aria-label={`Read ${document.paper.title}`}
                onClick={() =>
                  onAction({
                    kind: "document",
                    label: "Read",
                    revisionId: revision.id,
                  })
                }
              >
                Read
              </button>
              <button
                type="button"
                className="ui-button ui-button--link ui-button--sm"
                aria-label={`Compare ${document.paper.title}`}
                onClick={() =>
                  onAction({
                    kind: "document",
                    label: "Compare",
                    revisionId: revision.id,
                    mode: "compare",
                  })
                }
              >
                Compare
              </button>
            </div>
          </article>
        );
      })}
      {!documents.length && (
        <p className="overview-meta">
          Open your research folder or add a document to begin.
        </p>
      )}
    </div>
  );
}
