export interface OrientationMap {
  document_preparation?: {
    version: number;
    inventory_source: string;
    registry_revision: string;
  };
  // The backend stores the survey as raw JSON and validates it only as an
  // object for schema-less profiles, so even a paper-shaped survey can omit
  // any of these fields; treat every access as optional.
  metadata?: PaperMetadata;
  sections: SectionEntry[];
  formal_results: FormalResult[];
  tables_figures: TableFigure[];
  notation: NotationEntry[];
  stated_contribution: string;
  key_references: string[];
  extraction_quality_notes: ExtractionQualityNote[];
  review_plan?: ReviewPlan | null;
}

export interface ReviewSelectionNote {
  id: string;
  reason: string;
  references?: string[];
}

export interface ReviewPlan {
  primary_domain: string;
  subject: string;
  paper_forms: string[];
  methods: string[];
  subject_specialist_ids?: string[];
  method_specialist_ids: string[];
  /** Document-genre classification shared with every reviewer as context. */
  genre?: string;
  selection_notes: ReviewSelectionNote[];
  routing_uncertainty: string[];
}

export interface PaperMetadata {
  title?: string;
  authors?: string[];
  date: string | null;
  paper_type: "theory" | "empirical" | "mixed";
  page_count: number | null;
  has_appendix: boolean;
  has_online_appendix: boolean;
}

export interface SectionEntry {
  number: string;
  title: string;
  page_start: number | null;
  page_end: number | null;
}

export interface FormalResult {
  kind: string;
  number: string;
  page: number | null;
  summary: string;
  proof_location: string | null;
}

export interface TableFigure {
  kind: string;
  number: string;
  page: number | null;
  caption_summary: string;
  what_it_shows: string;
}

export interface NotationEntry {
  symbol: string;
  definition: string;
  page_introduced: number | null;
}

export interface ExtractionQualityNote {
  page_range: string;
  description: string;
}
