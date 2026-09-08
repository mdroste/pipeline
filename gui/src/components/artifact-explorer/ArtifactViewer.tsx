import { lazy, Suspense, memo } from "react";
import { artifactClient } from "../../lib/artifactClient";
import type {
  ArtifactEntry,
  ArtifactContent,
  PageArtifactIndex,
} from "../../lib/artifactTypes";
import ReportViewer from "../ReportViewer";
import { IMAGE_MIME, ext, formatBytes } from "./format";
import { BinaryCard, CodeView, CsvView } from "./ArtifactPreviews";
import { PdfView } from "./PdfArtifactPreview";
import { DocumentBundleView } from "./DocumentBundleView";
const PdfReader = lazy(() => import("../file-workspace/PdfReader"));
export const Viewer = memo(function Viewer({
  runId,
  entry,
  content,
  artifacts,
  pageArtifacts,
  onSelectArtifact,
}: {
  runId: string;
  entry: ArtifactEntry | null;
  content: ArtifactContent;
  artifacts: ArtifactEntry[];
  pageArtifacts?: PageArtifactIndex | null;
  onSelectArtifact: (relPath: string) => void;
}) {
  if (content.kind === "image") {
    if (!content.base64) return <BinaryCard entry={entry} content={content} />;
    const mime = IMAGE_MIME[ext(entry?.rel_path ?? "")] ?? "image/png";
    return (
      <div className="p-4 flex justify-center">
        <img
          src={`data:${mime};base64,${content.base64}`}
          alt={entry?.label ?? "artifact"}
          className="max-w-full h-auto"
        />
      </div>
    );
  }
  if (content.kind === "pdf") {
    return (
      <Suspense fallback={<p>Loading PDF reader…</p>}>
        <PdfReader
          documentKey={`${runId}:${entry?.rel_path}`}
          title={entry?.label ?? "PDF artifact"}
          load={() => artifactClient.pdfBytes(runId, entry?.rel_path)}
          fallback={
            <PdfView
              key={`${runId}:${entry?.rel_path}`}
              runId={runId}
              entry={entry}
              content={content}
            />
          }
        />
      </Suspense>
    );
  }
  if (content.kind === "binary" || content.text === null) {
    return <BinaryCard entry={entry} content={content} />;
  }

  const truncBanner = content.truncated && (
    <div className="mb-3 rounded border border-amber-300 bg-amber-50 px-3 py-2 text-xs text-amber-800 dark:border-amber-700 dark:bg-amber-950 dark:text-amber-200">
      Large file — showing the first {formatBytes(1_000_000)} of{" "}
      {formatBytes(content.bytes)}.
    </div>
  );

  switch (content.kind) {
    case "markdown":
      return (
        <div className="h-full flex flex-col">
          {truncBanner && <div className="px-4 pt-3">{truncBanner}</div>}
          <div className="flex-1 min-h-0 overflow-auto">
            <ReportViewer markdown={content.text} />
          </div>
        </div>
      );
    case "json": {
      if (
        entry?.rel_path === "context/document_bundle.json" &&
        !content.truncated
      ) {
        return (
          <DocumentBundleView
            text={content.text}
            artifacts={artifacts}
            pageArtifacts={pageArtifacts}
            onSelectArtifact={onSelectArtifact}
          />
        );
      }
      let pretty = content.text;
      if (!content.truncated) {
        try {
          pretty = JSON.stringify(JSON.parse(content.text), null, 2);
        } catch {
          // leave as-is
        }
      }
      return (
        <div className="p-4">
          {truncBanner}
          <CodeView text={pretty} relPath="artifact.json" />
        </div>
      );
    }
    case "csv":
      return (
        <div className="p-4">
          {truncBanner}
          <CsvView text={content.text} relPath={entry?.rel_path ?? ""} />
        </div>
      );
    case "code":
      return (
        <div className="p-4">
          {truncBanner}
          <CodeView text={content.text} relPath={entry?.rel_path ?? ""} />
        </div>
      );
    default:
      return (
        <div className="p-4">
          {truncBanner}
          <pre className="text-xs font-mono leading-relaxed whitespace-pre-wrap text-gray-800 dark:text-gray-200">
            {content.text}
          </pre>
        </div>
      );
  }
});
