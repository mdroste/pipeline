import { useMemo, useState } from "react";
import type {
  ArtifactEntry,
  PageArtifactIndex,
  DocumentBundle,
} from "../../lib/artifactTypes";
import { CodeView } from "./ArtifactPreviews";
import { indexedPageForPath, pageSelection } from "./navigation";
import {
  coalesceEquationNumberNodes,
  formatRepresentation,
  STRUCTURAL_KINDS,
  INITIAL_BUNDLE_NODES,
} from "./documentBundle";
export function DocumentBundleView({
  text,
  artifacts,
  pageArtifacts,
  onSelectArtifact,
}: {
  text: string;
  artifacts: ArtifactEntry[];
  pageArtifacts?: PageArtifactIndex | null;
  onSelectArtifact: (relPath: string) => void;
}) {
  const [kindFilter, setKindFilter] = useState("structure");
  const [nodeLimit, setNodeLimit] = useState(INITIAL_BUNDLE_NODES);
  const bundle = useMemo(() => {
    try {
      return JSON.parse(text) as DocumentBundle;
    } catch {
      return null;
    }
  }, [text]);
  const displayNodes = useMemo(
    () => coalesceEquationNumberNodes(bundle?.nodes ?? []),
    [bundle],
  );
  const counts = useMemo(() => {
    const result = new Map<string, number>();
    for (const node of displayNodes) {
      result.set(node.kind, (result.get(node.kind) ?? 0) + 1);
    }
    return result;
  }, [displayNodes]);
  const filters = useMemo(
    () => ["structure", "all", ...Array.from(counts.keys()).sort()],
    [counts],
  );
  const matchingNodes = useMemo(
    () =>
      displayNodes.filter((node) =>
        kindFilter === "structure"
          ? STRUCTURAL_KINDS.has(node.kind)
          : kindFilter === "all" || node.kind === kindFilter,
      ),
    [displayNodes, kindFilter],
  );
  const visibleNodes = matchingNodes.slice(0, nodeLimit);
  const artifactsByPath = useMemo(
    () => new Map(artifacts.map((artifact) => [artifact.rel_path, artifact])),
    [artifacts],
  );
  const assetById = useMemo(
    () => new Map((bundle?.assets ?? []).map((asset) => [asset.id, asset])),
    [bundle],
  );
  const visualAssets = useMemo(
    () => (bundle?.assets ?? []).filter((asset) => asset.kind !== "page"),
    [bundle],
  );

  if (!bundle) {
    return <CodeView text={text} relPath="document_bundle.json" />;
  }

  const summary = [
    ["Pages", bundle.pages?.length ?? 0],
    ["Blocks", displayNodes.length],
    ["Figures", counts.get("figure") ?? 0],
    ["Tables", counts.get("table") ?? 0],
    ["Equations", counts.get("equation") ?? 0],
    ["Assets", bundle.assets?.length ?? 0],
  ] as const;

  return (
    <div className="p-5 space-y-5 text-sm text-gray-800 dark:text-gray-200">
      <section>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h2 className="text-lg font-semibold">DocumentBundle inspection</h2>
            <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
              Schema {bundle.schema_version} · {bundle.source_kind} · extracted
              with {bundle.extraction?.method}
            </p>
          </div>
          <code className="rounded bg-gray-100 dark:bg-gray-800 px-2 py-1 text-[11px]">
            {bundle.bundle_id}
          </code>
        </div>
        <div className="mt-4 grid grid-cols-2 sm:grid-cols-3 xl:grid-cols-6 gap-2">
          {summary.map(([label, value]) => (
            <div
              key={label}
              className="rounded-lg border border-gray-200 dark:border-gray-700 p-3"
            >
              <div className="text-xl font-semibold">{value}</div>
              <div className="text-[11px] uppercase tracking-wide text-gray-500">
                {label}
              </div>
            </div>
          ))}
        </div>
      </section>

      <section className="grid gap-3 lg:grid-cols-2">
        <div className="rounded-lg border border-gray-200 dark:border-gray-700 p-3">
          <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
            Source provenance
          </h3>
          <div className="mt-2 space-y-2">
            {(bundle.origins ?? []).map((origin) => (
              <div key={origin.id}>
                <div className="font-medium">
                  {origin.role} · {origin.kind}
                </div>
                <div className="mt-0.5 break-all font-mono text-[11px] text-gray-500">
                  {origin.path}
                </div>
              </div>
            ))}
          </div>
        </div>
        <div className="rounded-lg border border-gray-200 dark:border-gray-700 p-3">
          <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
            Extraction quality
          </h3>
          {(bundle.quality ?? []).length === 0 ? (
            <p className="mt-2 text-xs text-emerald-700 dark:text-emerald-400">
              No extraction warnings recorded.
            </p>
          ) : (
            <ul className="mt-2 space-y-2">
              {bundle.quality.map((note, index) => (
                <li key={`${note.scope}-${index}`} className="text-xs">
                  <span className="font-medium">
                    {note.severity} · {note.scope}:
                  </span>{" "}
                  {note.message}
                </li>
              ))}
            </ul>
          )}
        </div>
      </section>

      <section>
        <h3 className="text-sm font-semibold">Visual assets</h3>
        {visualAssets.length === 0 ? (
          <p className="mt-2 text-xs text-gray-500">
            {bundle.pages?.length
              ? `${bundle.pages.length} page renders are indexed in the compact Pages browser; no separate figures or media were extracted.`
              : "No visual assets were extracted."}
          </p>
        ) : (
          <div className="mt-2 grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
            {visualAssets.map((asset) => {
              const indexedPage = indexedPageForPath(
                asset.rel_path,
                pageArtifacts,
              );
              const manifestArtifact = artifactsByPath.get(asset.rel_path);
              const displayLabel =
                manifestArtifact?.label.trim() || asset.label;
              const canOpen =
                manifestArtifact !== undefined || indexedPage !== null;
              return (
                <button
                  key={asset.id}
                  type="button"
                  disabled={!canOpen}
                  onClick={() =>
                    onSelectArtifact(
                      indexedPage === null
                        ? asset.rel_path
                        : pageSelection(indexedPage),
                    )
                  }
                  className="rounded-lg border border-gray-200 dark:border-gray-700 p-3 text-left
                             enabled:hover:border-gray-400 enabled:hover:bg-gray-50 dark:enabled:hover:bg-gray-800
                             disabled:opacity-60 transition-colors"
                >
                  <div className="flex justify-between gap-2">
                    <span className="font-medium truncate">{displayLabel}</span>
                    <span className="text-[10px] uppercase text-gray-500">
                      {asset.kind}
                    </span>
                  </div>
                  <div className="mt-1 text-[11px] text-gray-500">
                    {asset.page ? `Page ${asset.page} · ` : ""}
                    {asset.width && asset.height
                      ? `${asset.width}×${asset.height} · `
                      : ""}
                    {asset.media_type}
                  </div>
                  <div className="mt-1 truncate font-mono text-[10px] text-gray-600 dark:text-gray-400">
                    {asset.rel_path}
                  </div>
                  {!canOpen && (
                    <div className="mt-1 text-[10px] text-amber-700 dark:text-amber-300">
                      Asset missing from this report
                    </div>
                  )}
                </button>
              );
            })}
          </div>
        )}
      </section>

      <section>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h3 className="text-sm font-semibold">Extracted structure</h3>
          <div
            className="flex flex-wrap gap-1"
            aria-label="Document node filters"
          >
            {filters.map((kind) => (
              <button
                key={kind}
                type="button"
                onClick={() => {
                  setKindFilter(kind);
                  setNodeLimit(INITIAL_BUNDLE_NODES);
                }}
                className={`rounded-full border px-2 py-0.5 text-[11px] ${
                  kindFilter === kind
                    ? "border-gray-900 bg-gray-900 text-white dark:border-gray-100 dark:bg-gray-100 dark:text-gray-900"
                    : "border-gray-300 text-gray-600 dark:border-gray-700 dark:text-gray-300"
                }`}
              >
                {kind === "structure"
                  ? "Structural"
                  : kind === "all"
                    ? `All (${displayNodes.length})`
                    : `${kind} (${counts.get(kind) ?? 0})`}
              </button>
            ))}
          </div>
        </div>
        <p className="mt-1 text-xs text-gray-500">
          The readable document artifact contains the full extracted text. This
          view exposes semantic blocks and provenance.
        </p>
        <div className="mt-3 space-y-2">
          {visibleNodes.map((node) => (
            <details
              key={node.id}
              className="rounded-lg border border-gray-200 dark:border-gray-700"
            >
              <summary className="cursor-pointer list-none p-3">
                <div className="flex items-start justify-between gap-3">
                  <div>
                    <span className="font-medium">
                      {node.label || node.kind}
                    </span>
                    {node.number && (
                      <span className="ml-1 text-gray-500">
                        ({node.number})
                      </span>
                    )}
                    <div className="mt-0.5 text-[11px] text-gray-500">
                      {node.id}
                      {node.page ? ` · page ${node.page}` : ""} ·{" "}
                      {node.provenance?.method}
                    </div>
                  </div>
                  <span className="rounded bg-gray-100 dark:bg-gray-800 px-1.5 py-0.5 text-[10px] uppercase">
                    {node.kind}
                  </span>
                </div>
                {node.text && (
                  <p className="mt-2 line-clamp-3 whitespace-pre-wrap text-xs text-gray-600 dark:text-gray-300">
                    {node.text.length > 800
                      ? `${node.text.slice(0, 800)}…`
                      : node.text}
                  </p>
                )}
              </summary>
              <div className="border-t border-gray-200 dark:border-gray-700 p-3 space-y-3">
                {node.text && (
                  <pre className="whitespace-pre-wrap text-xs font-sans leading-relaxed">
                    {node.text}
                  </pre>
                )}
                {node.asset_ids?.length > 0 && (
                  <div className="flex flex-wrap gap-1">
                    {node.asset_ids.map((assetId) => {
                      const asset = assetById.get(assetId);
                      const indexedPage = asset
                        ? indexedPageForPath(asset.rel_path, pageArtifacts)
                        : null;
                      return (
                        <button
                          key={assetId}
                          type="button"
                          disabled={
                            !asset ||
                            (!artifactsByPath.has(asset.rel_path) &&
                              indexedPage === null)
                          }
                          onClick={() =>
                            asset &&
                            onSelectArtifact(
                              indexedPage === null
                                ? asset.rel_path
                                : pageSelection(indexedPage),
                            )
                          }
                          className="rounded border border-gray-300 dark:border-gray-600 px-2 py-1 text-[11px] disabled:opacity-50"
                        >
                          View {asset?.label ?? assetId}
                        </button>
                      );
                    })}
                  </div>
                )}
                {node.representations?.map((representation, index) => (
                  <details key={`${representation.format}-${index}`}>
                    <summary className="cursor-pointer text-xs font-medium text-gray-500">
                      {representation.format} representation
                    </summary>
                    <pre className="mt-1 overflow-auto rounded bg-gray-50 dark:bg-gray-900 p-2 text-[11px] whitespace-pre-wrap">
                      {formatRepresentation(representation.content)}
                    </pre>
                  </details>
                ))}
              </div>
            </details>
          ))}
          {visibleNodes.length === 0 && (
            <p className="rounded border border-dashed border-gray-300 dark:border-gray-700 p-4 text-xs text-gray-500">
              No nodes match this filter.
            </p>
          )}
          {visibleNodes.length < matchingNodes.length && (
            <button
              type="button"
              onClick={() =>
                setNodeLimit((limit) => limit + INITIAL_BUNDLE_NODES)
              }
              className="w-full rounded-lg border border-gray-200 px-3 py-2 text-xs text-gray-500
                         hover:border-gray-400 hover:text-gray-700 dark:border-gray-700 dark:hover:border-gray-500
                         dark:hover:text-gray-200"
            >
              Show{" "}
              {Math.min(
                INITIAL_BUNDLE_NODES,
                matchingNodes.length - visibleNodes.length,
              )}{" "}
              more ({matchingNodes.length - visibleNodes.length} remaining)
            </button>
          )}
        </div>
      </section>
    </div>
  );
}
