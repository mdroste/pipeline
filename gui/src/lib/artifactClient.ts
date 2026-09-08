import { invoke } from "@tauri-apps/api/core";
import type {
  ArtifactContent,
  PdfArtifactPage,
  RunManifest,
} from "./artifactTypes";

/** Reads remain authorized and bounded by the owning Workflow backend. */
export const artifactClient = {
  manifest: (runId: string) =>
    invoke<RunManifest>("get_run_manifest", { runId }),
  read: (runId: string, relPath: string) =>
    invoke<ArtifactContent>("read_artifact", { runId, relPath }),
  page: (runId: string, page: number) =>
    invoke<ArtifactContent>("read_page_artifact", { runId, page }),
  pdfPage: (runId: string, relPath: string, page: number) =>
    invoke<PdfArtifactPage>("read_pdf_artifact_page", { runId, relPath, page }),
  pdfBytes: (runId: string, relPath?: string) =>
    invoke<string>("read_pdf_artifact_bytes", { runId, relPath }),
};
