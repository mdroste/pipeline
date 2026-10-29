import { reviewHistoryClient } from "../../lib/reviewHistoryClient";
import { useEffect, useState, type Dispatch, type SetStateAction } from "react";
import { artifactClient } from "../../lib/artifactClient";
import type {
  ArtifactSelectionRequest,
  ArtifactSelectionTarget,
  RunManifest,
} from "../../lib/artifactTypes";
import type { PipelineReport } from "../../lib/types";
import type { OutputView, WorkspaceTab } from "./types";

interface ArtifactContent {
  text: string | null;
}
interface Props {
  runId?: string | null;
  initialReport: PipelineReport | null;
  initialMarkdown: string;
  initialSourceSelection: ArtifactSelectionTarget | null;
  setSourceRequest: Dispatch<SetStateAction<ArtifactSelectionRequest | null>>;
  setTab: Dispatch<SetStateAction<WorkspaceTab>>;
  setOutputView: Dispatch<SetStateAction<OutputView>>;
}
export default function useReportResources({
  runId,
  initialReport,
  initialMarkdown,
  initialSourceSelection,
  setSourceRequest,
  setTab,
  setOutputView,
}: Props) {
  const [report, setReport] = useState<PipelineReport | null>(initialReport);
  const [markdown, setMarkdown] = useState(initialMarkdown);
  const [loading, setLoading] = useState(
    Boolean(runId && (!initialReport || !initialMarkdown)),
  );
  const [reportError, setReportError] = useState<string | null>(null);
  const [markdownError, setMarkdownError] = useState<string | null>(null);
  const [runManifest, setRunManifest] = useState<RunManifest | null>(null);

  useEffect(() => {
    setRunManifest(null);
    setSourceRequest(
      initialSourceSelection
        ? { key: Date.now(), ...initialSourceSelection }
        : null,
    );
    if (!runId) return;
    let live = true;

    // The compact manifest supports provenance and evidence links. The source
    // tree component and artifact bytes remain truly lazy until Sources opens.
    void artifactClient
      .manifest(runId)
      .then((loaded) => {
        if (live) setRunManifest(loaded);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [initialSourceSelection, runId]);

  useEffect(() => {
    setReport(initialReport);
    setMarkdown(initialMarkdown);
    setTab(initialSourceSelection ? "sources" : "report");
    setOutputView("clean");
    setReportError(null);
    setMarkdownError(null);
    if (!runId || (initialReport && initialMarkdown)) {
      setLoading(false);
      return;
    }

    let live = true;
    setLoading(true);
    Promise.allSettled([
      initialReport
        ? Promise.resolve(initialReport)
        : reviewHistoryClient.report({ runId }),
      initialMarkdown
        ? Promise.resolve({ text: initialMarkdown } as ArtifactContent)
        : artifactClient.read(runId, "report.md"),
    ])
      .then(([loadedReport, artifact]) => {
        if (!live) return;
        if (loadedReport.status === "fulfilled") {
          setReport(loadedReport.value);
          setReportError(null);
        } else {
          setReportError(
            loadedReport.reason instanceof Error
              ? loadedReport.reason.message
              : String(loadedReport.reason),
          );
        }
        if (artifact.status === "fulfilled") {
          setMarkdown(artifact.value.text ?? "");
          setMarkdownError(null);
        } else {
          setMarkdownError(
            artifact.reason instanceof Error
              ? artifact.reason.message
              : String(artifact.reason),
          );
          // The artifact explorer is independent of both canonical report
          // files, so make the recoverable material the default destination.
          setTab("sources");
        }
      })
      .finally(() => {
        if (live) setLoading(false);
      });
    return () => {
      live = false;
    };
  }, [initialMarkdown, initialReport, initialSourceSelection, runId]);

  return { report, markdown, loading, reportError, markdownError, runManifest };
}
