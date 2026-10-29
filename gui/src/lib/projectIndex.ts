// Typed client and helpers for the Projects index (`workbench_project_index`).
// The backend reads bounded metadata only and never loads transcripts,
// artifacts or model runtimes, so the index stays fast with many projects.
// Pins are a device preference in localStorage; they change no research record
// or permission.

import { invoke } from "@tauri-apps/api/core";

export interface ProjectIndexEntry {
  id: string;
  title: string;
  updatedAt: string;
  kind: string;
}

export interface ProjectIndexItem {
  id: string;
  name: string;
  root: string | null;
  missingRootAt: string | null;
  updatedAt: string;
  brief: ProjectIndexEntry | null;
  conversation: ProjectIndexEntry | null;
  activity: ProjectIndexEntry | null;
  nextTask: ProjectIndexEntry | null;
  proposedNotes: number;
  interruptedEdits: number;
}

export const listProjectIndex = () =>
  invoke<ProjectIndexItem[]>("workbench_project_index");

export function projectDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : date.toLocaleString(undefined, {
        dateStyle: "medium",
        timeStyle: "short",
      });
}

export const activityKind = (kind: string) =>
  ({
    conversation: "Conversation",
    document: "Document",
    note: "Saved note",
    task: "Action item",
  })[kind] ?? "Project";

const PIN_KEY = "pipeline.projects.pinned";
export function loadProjectPins(): string[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(PIN_KEY) ?? "[]");
    return Array.isArray(value)
      ? [
          ...new Set(
            value.filter((id): id is string => typeof id === "string"),
          ),
        ].slice(0, 500)
      : [];
  } catch {
    return [];
  }
}
export function saveProjectPins(pins: string[]) {
  localStorage.setItem(PIN_KEY, JSON.stringify(pins));
}
