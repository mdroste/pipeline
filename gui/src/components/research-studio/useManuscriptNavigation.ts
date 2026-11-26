/** Source/PDF position and SyncTeX mapping. File transitions use an intent. */
import { useState, useRef } from "react";
import type { PdfPosition } from "../file-workspace/PdfReader";
import type { SourceLineRequest } from "../file-workspace/SourceEditor";
import { studioClient } from "../../lib/studioClient";
import type { BuildReceipt } from "../../lib/studioTypes";
import type { useAction } from "./shared";
export function useManuscriptNavigation({
  workspaceId,
  receipt,
  prefix,
  run,
  navigateSource,
}: {
  workspaceId: string;
  receipt: BuildReceipt | undefined;
  prefix: string;
  run: ReturnType<typeof useAction>["run"];
  navigateSource: (target: string, line: number) => Promise<void>;
}) {
  const [sync, setSync] = useState("");
  const [syncCandidates, setSyncCandidates] = useState<
    import("../../lib/studioClient").SyncResult["candidates"]
  >([]);
  // The Source line field stays local; only explicit actions move the editor.
  const [sourceLine, setSourceLine] = useState("1");
  const [sourcePassage, setSourcePassage] = useState<{
    start: number;
    end: number;
    line: number;
    quote: string;
  } | null>(null);
  const [sourceRequest, setSourceRequest] = useState<SourceLineRequest | null>(
    null,
  );
  const [pdfPage, setPdfPage] = useState(1);
  const pdfPosition = useRef<PdfPosition>({ page: 1 });
  const showSync = async (request: {
    sourcePath?: string;
    line?: number;
    page?: number;
    x?: number;
    y?: number;
  }) => {
    if (!receipt) return;
    const mapped = await studioClient.sync(
      workspaceId,
      receipt.executionId,
      request.sourcePath?.startsWith(prefix)
        ? { ...request, sourcePath: request.sourcePath.slice(prefix.length) }
        : request,
    );
    setSync(mapped.result);
    setSyncCandidates(mapped.candidates);
    if (mapped.candidates[0]?.page) setPdfPage(mapped.candidates[0].page);
  };
  // Reveal a line without taking focus from the control that asked for it.
  const revealLine = (line: number) => {
    setSourceLine(String(line));
    setSourceRequest({ line });
  };
  /** Follow a click in the PDF back to its source line. */
  const fromPdf = (point: { page: number; x: number; y: number }) =>
    run(async () => {
      if (!receipt) return;
      const mapped = await studioClient.sync(
        workspaceId,
        receipt.executionId,
        point,
      );
      setSyncCandidates(mapped.candidates);
      const first = mapped.candidates.find((c) => c.sourcePath && c.line);
      if (first) await navigateSource(prefix + first.sourcePath!, first.line!);
    });
  return {
    sync,
    syncCandidates,
    sourceLine,
    setSourceLine,
    sourcePassage,
    setSourcePassage,
    sourceRequest,
    pdfPage,
    setPdfPage,
    pdfPosition,
    showSync,
    revealLine,
    fromPdf,
  };
}
