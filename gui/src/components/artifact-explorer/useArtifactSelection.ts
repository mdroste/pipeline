import { useCallback, useEffect, useMemo, useState } from "react";
import { artifactClient } from "../../lib/artifactClient";
import type {
  ArtifactContent,
  ArtifactEntry,
  ArtifactExplorerProps,
  RunManifest,
} from "../../lib/artifactTypes";
import type { FileLocation } from "../../lib/fileLinks";
import {
  indexedPageForPath,
  pageNumber,
  pageSelection,
  selectedPage,
} from "./navigation";
export default function useArtifactSelection({
  runId,
  deferInitialArtifact = false,
  preload = null,
  selectionRequest = null,
}: Pick<
  ArtifactExplorerProps,
  "runId" | "deferInitialArtifact" | "preload" | "selectionRequest"
>) {
  const [fileTab, setFileTab] = useState<FileLocation | null>(null);
  const [manifest, setManifest] = useState<RunManifest | null>(null);
  const [manifestError, setManifestError] = useState<string | null>(null);
  const [manifestAttempt, setManifestAttempt] = useState(0);
  const [selected, setSelected] = useState<string>(
    deferInitialArtifact ? "" : "report.md",
  );
  const [selectionHistory, setSelectionHistory] = useState<string[]>([]);
  const [content, setContent] = useState<ArtifactContent | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [navigationOpen, setNavigationOpen] = useState(true);

  useEffect(() => {
    let live = true;
    // Clear every run-scoped value before requesting the next manifest. This
    // also prevents the content effect from combining a new run ID with the
    // previous run's selection.
    setManifest(null);
    setManifestError(null);
    setSelected(deferInitialArtifact ? "" : "report.md");
    setSelectionHistory([]);
    setContent(null);
    setLoadError(null);
    const request =
      preload?.runId === runId
        ? preload.manifest.catch(() => artifactClient.manifest(runId))
        : artifactClient.manifest(runId);
    request
      .then((m) => {
        if (live) {
          const initial = deferInitialArtifact
            ? ""
            : m.artifacts.some((artifact) => artifact.rel_path === "report.md")
              ? "report.md"
              : (m.artifacts[0]?.rel_path ?? "");
          setSelected(initial);
          setManifest(m);
        }
      })
      .catch((e) => {
        console.error("Failed to load run manifest:", e);
        if (live) setManifestError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      live = false;
    };
  }, [deferInitialArtifact, preload, runId, manifestAttempt]);

  useEffect(() => {
    if (!manifest || !selected) return;
    let live = true;
    setContent(null);
    setLoadError(null);
    const compactPage = manifest.page_artifacts ? selectedPage(selected) : null;
    const preloadedReadable =
      preload?.runId === runId && selected === "context/document.md"
        ? preload.readableDocument
        : null;
    const request = preloadedReadable
      ? preloadedReadable.catch(() => artifactClient.read(runId, selected))
      : compactPage === null
        ? artifactClient.read(runId, selected)
        : artifactClient.page(runId, compactPage);
    request
      .then((c) => {
        if (live) setContent(c);
      })
      .catch((e) => {
        console.error("Failed to read artifact:", e);
        if (live) {
          setContent(null);
          setLoadError(e instanceof Error ? e.message : String(e));
        }
      });
    return () => {
      live = false;
    };
  }, [manifest, preload, runId, selected]);

  const artifactsByGroup = useMemo(() => {
    const grouped = new Map<string, ArtifactEntry[]>();
    for (const artifact of manifest?.artifacts ?? []) {
      const items = grouped.get(artifact.group);
      if (items) {
        items.push(artifact);
      } else {
        grouped.set(artifact.group, [artifact]);
      }
    }
    return grouped;
  }, [manifest]);
  const artifactsByPath = useMemo(
    () =>
      new Map(
        (manifest?.artifacts ?? []).map((artifact) => [
          artifact.rel_path,
          artifact,
        ]),
      ),
    [manifest],
  );
  const selectArtifact = useCallback(
    (next: string) => {
      if (!next || next === selected) return;
      if (selected) {
        setSelectionHistory((history) => [...history.slice(-99), selected]);
      }
      setSelected(next);
    },
    [selected],
  );
  useEffect(() => {
    if (!manifest || !selectionRequest) return;
    let next = "";
    if (selectionRequest.relPath) {
      const indexedPage = indexedPageForPath(
        selectionRequest.relPath,
        manifest.page_artifacts,
      );
      if (indexedPage !== null) {
        next = pageSelection(indexedPage);
      } else if (
        manifest.artifacts.some(
          (artifact) => artifact.rel_path === selectionRequest.relPath,
        )
      ) {
        next = selectionRequest.relPath;
      }
    }
    if (!next && selectionRequest.page && selectionRequest.page > 0) {
      if (
        manifest.page_artifacts &&
        selectionRequest.page <= manifest.page_artifacts.count
      ) {
        next = pageSelection(selectionRequest.page);
      } else {
        next =
          manifest.artifacts.find(
            (artifact) =>
              artifact.group === "pages" &&
              pageNumber(artifact) === selectionRequest.page,
          )?.rel_path ?? "";
      }
    }
    if (
      next &&
      selectionRequest.relPath &&
      (selectionRequest.line ||
        selectionRequest.fragment ||
        next.endsWith(".pdf"))
    )
      setFileTab({
        path: next,
        line: selectionRequest.line,
        page: selectionRequest.page,
        fragment: selectionRequest.fragment,
      });
    if (next) {
      setNavigationOpen(true);
      selectArtifact(next);
    }
  }, [manifest, selectArtifact, selectionRequest]);
  const goBack = useCallback(() => {
    const previous = selectionHistory[selectionHistory.length - 1];
    if (!previous) return;
    setSelectionHistory((history) => history.slice(0, -1));
    setSelected(previous);
  }, [selectionHistory]);

  const pageIndex = manifest?.page_artifacts ?? null;
  const compactPage = pageIndex ? selectedPage(selected) : null;
  const averagePageBytes =
    pageIndex && pageIndex.count > 0
      ? Math.round(pageIndex.total_bytes / pageIndex.count)
      : 0;
  const entry =
    compactPage === null
      ? (artifactsByPath.get(selected) ?? null)
      : {
          rel_path: `artifacts/pages/page-${compactPage}.${pageIndex?.extension ?? "jpg"}`,
          label: `Page ${compactPage}`,
          kind: "image",
          bytes: averagePageBytes,
          sha256: "",
          group: "pages",
        };

  return {
    fileTab,
    setFileTab,
    manifest,
    manifestError,
    retryManifest: () => setManifestAttempt((attempt) => attempt + 1),
    selected,
    selectionHistory,
    content,
    loadError,
    navigationOpen,
    setNavigationOpen,
    artifactsByGroup,
    selectArtifact,
    goBack,
    entry,
  };
}
