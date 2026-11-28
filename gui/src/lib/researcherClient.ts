import { invoke } from "@tauri-apps/api/core";

/** What kind of writing a sample is; matches a project's Kind. */
export type WritingKind = "paper" | "grant" | "response" | "other";

/** A finished piece of the researcher's writing kept in About you. */
export interface WritingSample {
  id: string;
  title: string;
  year: number | null;
  kind: WritingKind;
  /** The name of the file that was added. */
  fileName: string;
  extension: string;
  contentHash: string;
  /** Words of extracted text, or null when none could be read. */
  words: number | null;
  useAsExample: boolean;
  addedAt: string;
}

/** About you: what Pipeline knows about the researcher in every project. */
export interface ResearcherProfile {
  /** The background note sent with conversations. */
  about: string;
  style: string;
  samples: WritingSample[];
  folder: string;
  maxNoteBytes: number;
  maxSamples: number;
}

export interface WritingSampleUpdate {
  id: string;
  title: string;
  year: number | null;
  kind: WritingKind;
  useAsExample: boolean;
}

/** Style notes a model drafted from the papers marked as style examples. */
export interface StyleDraft {
  text: string;
  /** Titles of the papers it was drafted from. */
  papers: string[];
}

// Every change returns the profile as saved.
export const researcherClient = {
  profile: () => invoke<ResearcherProfile>("researcher_profile"),
  saveNote: (args: { note: "about" | "style"; text: string }) =>
    invoke<ResearcherProfile>("researcher_save_note", args),
  addSample: (args: { path: string }) =>
    invoke<ResearcherProfile>("researcher_add_sample", args),
  updateSample: (args: { update: WritingSampleUpdate }) =>
    invoke<ResearcherProfile>("researcher_update_sample", args),
  removeSample: (args: { id: string }) =>
    invoke<ResearcherProfile>("researcher_remove_sample", args),
  openFolder: () => invoke<void>("researcher_open_folder"),
  /** One model call on the default connection; nothing is saved. */
  draftStyle: () => invoke<StyleDraft>("workbench_draft_style_notes"),
};
