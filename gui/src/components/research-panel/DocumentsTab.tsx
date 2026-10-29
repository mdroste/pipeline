import { open } from "@tauri-apps/plugin-dialog";
import { promptDialog } from "../DialogService";
import { researchClient } from "../../lib/workbench/researchClient";
import { operation } from "./useResearchPanelController";
import { compactHash } from "./format";
import type { useResearchPanelController } from "./useResearchPanelController";
import type { Workspace } from "../../lib/workbenchTypes";

type Panel = ReturnType<typeof useResearchPanelController>;
type Props = Pick<
  Panel,
  | "busy"
  | "act"
  | "papers"
  | "sources"
  | "selectedPaper"
  | "setProjectPaperId"
  | "patchSession"
  | "query"
  | "setQuery"
  | "hits"
  | "setHits"
> & {
  workspaceId: string;
  projectWorkspace?: Workspace | null;
  onError: (message: string) => void;
};

export default function DocumentsTab({
  busy,
  act,
  papers,
  sources,
  selectedPaper,
  setProjectPaperId,
  patchSession,
  query,
  setQuery,
  hits,
  setHits,
  workspaceId,
  projectWorkspace,
  onError,
}: Props) {
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          disabled={busy}
          onClick={() =>
            void act(async () => {
              const path = await open({
                multiple: false,
                filters: [
                  {
                    name: "Research documents",
                    extensions: ["pdf", "tex", "docx", "md", "txt", "bib"],
                  },
                ],
              });
              if (typeof path !== "string") return;
              const title = (
                await promptDialog("Name this paper in the project.", {
                  title: "Import paper",
                  label: "Paper title",
                  defaultValue:
                    path
                      .split(/[\\/]/)
                      .pop()
                      ?.replace(/\.[^.]+$/, "") ?? "Paper",
                  confirmLabel: "Import",
                })
              )?.trim();
              if (!title) return;
              const result = await researchClient.importPaper({
                workspaceId,
                paperId: null,
                title,
                role: "manuscript",
                path,
                operationId: operation("paper-import"),
              });
              setProjectPaperId(result.paper.id);
              if (!projectWorkspace)
                await patchSession({ paperId: result.paper.id });
            })
          }
          className="rounded bg-gray-900 px-3 py-2 text-xs font-medium text-white dark:bg-neutral-100 dark:text-neutral-900"
        >
          Import paper
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() =>
            void act(async () => {
              const path = await open({
                multiple: false,
                directory: true,
              });
              if (typeof path !== "string") return;
              const title = (
                await promptDialog("Name this source tree in the project.", {
                  title: "Import source tree",
                  label: "Source-tree title",
                  defaultValue: path.split(/[\\/]/).pop() ?? "TeX project",
                  confirmLabel: "Import",
                })
              )?.trim();
              if (!title) return;
              const result = await researchClient.importPaper({
                workspaceId,
                paperId: null,
                title,
                role: "manuscript",
                path,
                operationId: operation("tree-import"),
              });
              setProjectPaperId(result.paper.id);
              if (!projectWorkspace)
                await patchSession({ paperId: result.paper.id });
            })
          }
          className="rounded border px-3 py-2 text-xs"
        >
          Import source tree
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() =>
            void act(async () => {
              const path = await open({
                multiple: false,
                filters: [
                  {
                    name: "Sources",
                    extensions: ["pdf", "bib", "txt", "md"],
                  },
                ],
              });
              if (typeof path !== "string") return;
              const title = (
                await promptDialog("Name this source in the project.", {
                  title: "Import source",
                  label: "Source title",
                  defaultValue:
                    path
                      .split(/[\\/]/)
                      .pop()
                      ?.replace(/\.[^.]+$/, "") ?? "Source",
                  confirmLabel: "Import",
                })
              )?.trim();
              if (!title) return;
              const result = await researchClient.importSource({
                workspaceId,
                title,
                citationKey: null,
                identifiers: {},
                versionLabel: null,
                path,
                locator: null,
                accessState: "full",
                acquiredVia: path.toLowerCase().endsWith(".pdf")
                  ? "local_pdf"
                  : path.toLowerCase().endsWith(".bib")
                    ? "local_bibtex"
                    : "local_file",
                operationId: operation("source-import"),
              });
              if (result.duplicateCandidates.length)
                onError(
                  `Source imported separately; ${result.duplicateCandidates.length} possible duplicate${result.duplicateCandidates.length === 1 ? "" : "s"} retained for review.`,
                );
            })
          }
          className="rounded border px-3 py-2 text-xs"
        >
          Import source
        </button>
      </div>
      <section>
        <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
          Papers and versions
        </h3>
        <div className="mt-2 space-y-2">
          {papers.map((item) => (
            <button
              key={item.paper.id}
              type="button"
              onClick={() =>
                void act(async () => {
                  setProjectPaperId(item.paper.id);
                  if (!projectWorkspace)
                    await patchSession({ paperId: item.paper.id });
                })
              }
              className={`block w-full rounded border p-3 text-left ${item.paper.id === selectedPaper?.paper.id ? "border-blue-400 bg-blue-50 dark:bg-blue-950/20" : "bg-white dark:bg-neutral-950"}`}
            >
              <span className="block font-medium">{item.paper.title}</span>
              <span className="block text-[11px] text-gray-500">
                {item.paper.role} · {item.revision?.inputKind ?? "no revision"}{" "}
                · {compactHash(item.revision?.contentHash)}
              </span>
              <span
                className={`mt-1 block text-[11px] ${item.revision?.extraction.status === "failed" ? "text-red-600" : "text-gray-500"}`}
              >
                Extraction:{" "}
                {String(item.revision?.extraction.status ?? "unavailable")}
                {item.revision?.extraction.error
                  ? ` — ${String(item.revision.extraction.error)}`
                  : ""}
              </span>
            </button>
          ))}
        </div>
      </section>
      {selectedPaper?.revision && (
        <section>
          <div className="flex gap-2">
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search this version"
              className="min-w-0 flex-1 rounded border bg-white p-2 text-xs dark:bg-neutral-950"
            />
            <button
              type="button"
              disabled={!query.trim()}
              onClick={() =>
                void act(async () => {
                  setHits(
                    await researchClient.paperSearch(
                      workspaceId,
                      selectedPaper.revision!.id,
                      query,
                      20,
                    ),
                  );
                })
              }
              className="rounded border px-3 text-xs"
            >
              Search
            </button>
          </div>
          <div className="mt-2 space-y-2">
            {hits.map((hit) => (
              <details
                key={`${hit.start}-${hit.end}`}
                className="rounded border bg-white p-2 text-xs dark:bg-neutral-950"
              >
                <summary className="cursor-pointer">
                  {hit.page ? `p. ${hit.page}` : `line ${hit.line}`} · bytes{" "}
                  {hit.start}–{hit.end}
                </summary>
                <p className="mt-2 whitespace-pre-wrap text-gray-600 dark:text-neutral-300">
                  {hit.excerpt}
                </p>
              </details>
            ))}
          </div>
        </section>
      )}
      <section>
        <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
          Sources
        </h3>
        <div className="mt-2 space-y-2">
          {sources.map((source) => (
            <div
              key={source.id}
              className="rounded border bg-white p-2 dark:bg-neutral-950"
            >
              <span className="font-medium">{source.title}</span>
              <span className="block text-[11px] text-gray-500">
                {source.accessState} · {source.acquiredVia} ·{" "}
                {compactHash(source.contentHash)}
              </span>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
