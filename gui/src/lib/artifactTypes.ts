// App-owned Workflow artifact contracts; no component or runtime imports.

export interface ArtifactEntry {
  rel_path: string;
  label: string;
  kind: string;
  bytes: number;
  sha256: string;
  group: string;
}

export interface PageArtifactIndex {
  count: number;
  digit_width: number;
  extension: string;
  total_bytes: number;
}

export interface RunManifest {
  artifact_schema_version?: number;
  run_id: string;
  created: string;
  input_path: string;
  input_mode: string;
  profile_id?: string;
  profile_name: string;
  specialist_catalog_revision?: string;
  provider: string;
  artifacts: ArtifactEntry[];
  page_artifacts?: PageArtifactIndex | null;
  status?: string;
  duration_secs?: number;
  usage?: {
    input_tokens?: number;
    output_tokens?: number;
    cached_input_tokens?: number;
    cache_write_input_tokens?: number;
  };
  title?: string;
}

export interface ArtifactContent {
  kind: string;
  bytes: number;
  text: string | null;
  base64: string | null;
  truncated: boolean;
  abs_path: string;
}

export interface PdfArtifactPagePreview {
  page: number;
  has_previous: boolean;
  has_next: boolean;
  base64: string;
}

export interface PdfArtifactPage extends PdfArtifactPagePreview {
  prefetched_next?: PdfArtifactPagePreview | null;
}

export interface DocumentRepresentation {
  format: string;
  content: unknown;
}

export interface DocumentNode {
  id: string;
  kind: string;
  order: number;
  page?: number;
  parent_id?: string;
  label?: string;
  number?: string;
  text: string;
  asset_ids: string[];
  representations: DocumentRepresentation[];
  provenance: {
    origin_id: string;
    method: string;
    confidence?: number;
  };
}

export interface DocumentAsset {
  id: string;
  kind: string;
  label: string;
  rel_path: string;
  media_type: string;
  page?: number;
  width?: number;
  height?: number;
  provenance: {
    origin_id: string;
    method: string;
    confidence?: number;
  };
}

export interface DocumentBundle {
  schema_version: string;
  bundle_id: string;
  source_kind: string;
  origins: Array<{ id: string; kind: string; path: string; role: string }>;
  pages: Array<{
    number: number;
    label: string;
    asset_id?: string;
    width?: number;
    height?: number;
  }>;
  nodes: DocumentNode[];
  assets: DocumentAsset[];
  links?: Array<{ from_id: string; to_id: string; kind: string }>;
  extraction: { method: string; source_path: string; paper_hash: string };
  quality: Array<{ severity: string; scope: string; message: string }>;
}

export interface ArtifactExplorerProps {
  runId: string;
  /** Rendered if the manifest can't be loaded (run dir missing, etc.). */
  fallbackMarkdown: string;
  /** Load only the manifest until the user explicitly chooses an artifact. */
  deferInitialArtifact?: boolean;
  /** Background work started by the report workspace before Sources opens. */
  preload?: ArtifactExplorerPreload | null;
  /** External navigation request, for example from an evidence-linked issue. */
  selectionRequest?: ArtifactSelectionRequest | null;
}

export interface ArtifactSelectionRequest {
  key: number;
  page?: number;
  relPath?: string;
  line?: number;
  fragment?: string;
}

export type ArtifactSelectionTarget = Omit<ArtifactSelectionRequest, "key">;

export interface ArtifactExplorerPreload {
  runId: string;
  manifest: Promise<RunManifest>;
  readableDocument: Promise<ArtifactContent>;
}
