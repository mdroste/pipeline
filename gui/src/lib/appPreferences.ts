import { useSyncExternalStore } from "react";

/** Device presentation preferences. Never part of a Review or conversation runtime. */
export interface AppPreferences {
  version: 1;
  interfaceScale: number;
  reportScale: number;
  editorFontSize: number;
  editorWrap: boolean;
  density: "comfortable" | "compact";
  sendShortcut: "enter" | "mod-enter";
  startup: "restore" | "home";
  notifyCompletion: boolean;
  notifyFailure: boolean;
  notifyAttention: boolean;
  notificationSound: boolean;
  suppressFocused: boolean;
  desktopNotifications: boolean;
}

export const PREFERENCES_KEY = "pipeline.ui.preferences";
export const DEFAULT_PREFERENCES: AppPreferences = {
  version: 1,
  interfaceScale: 100,
  reportScale: 100,
  editorFontSize: 13,
  editorWrap: false,
  density: "comfortable",
  sendShortcut: "enter",
  startup: "restore",
  notifyCompletion: true,
  notifyFailure: true,
  notifyAttention: true,
  notificationSound: false,
  suppressFocused: false,
  desktopNotifications: false,
};

export function parsePreferences(raw: string | null): AppPreferences {
  let value: Record<string, unknown> = {};
  try {
    const parsed: unknown = JSON.parse(raw ?? "{}");
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed))
      value = parsed as Record<string, unknown>;
  } catch {
    /* Invalid optional preferences fall back to usable defaults. */
  }
  const next = { ...DEFAULT_PREFERENCES };
  for (const key of [
    "editorWrap",
    "notifyCompletion",
    "notifyFailure",
    "notifyAttention",
    "notificationSound",
    "suppressFocused",
    "desktopNotifications",
  ] as const)
    if (typeof value[key] === "boolean") next[key] = value[key];
  for (const [key, min, max] of [
    ["interfaceScale", 90, 125],
    ["reportScale", 85, 150],
    ["editorFontSize", 11, 24],
  ] as const)
    if (typeof value[key] === "number" && Number.isFinite(value[key]))
      next[key] = Math.max(min, Math.min(max, Math.round(value[key])));
  if (value.density === "compact") next.density = "compact";
  if (value.sendShortcut === "mod-enter") next.sendShortcut = "mod-enter";
  if (value.startup === "home") next.startup = "home";
  return next;
}

let cachedRaw: string | null | undefined;
let cached = DEFAULT_PREFERENCES;
const listeners = new Set<() => void>();

export function readAppPreferences(): AppPreferences {
  let raw: string | null = null;
  try {
    raw = localStorage.getItem(PREFERENCES_KEY);
  } catch {
    /* Read-only storage. */
  }
  if (raw !== cachedRaw) {
    cachedRaw = raw;
    cached = parsePreferences(raw);
  }
  return cached;
}

export function saveAppPreferences(
  patch: Partial<Omit<AppPreferences, "version">>,
) {
  const next = parsePreferences(
    JSON.stringify({ ...readAppPreferences(), ...patch }),
  );
  // Persist first: failed writes must never be represented as saved preferences.
  localStorage.setItem(PREFERENCES_KEY, JSON.stringify(next));
  cachedRaw = undefined;
  listeners.forEach((listener) => listener());
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  const changed = (event: StorageEvent) => {
    if (event.key === PREFERENCES_KEY || event.key === null) listener();
  };
  window.addEventListener("storage", changed);
  return () => {
    listeners.delete(listener);
    window.removeEventListener("storage", changed);
  };
}

export function useAppPreferences() {
  return useSyncExternalStore(
    subscribe,
    readAppPreferences,
    () => DEFAULT_PREFERENCES,
  );
}

export function shouldSendMessage(
  event: {
    key: string;
    shiftKey: boolean;
    metaKey: boolean;
    ctrlKey: boolean;
    altKey: boolean;
    isComposing: boolean;
  },
  shortcut: AppPreferences["sendShortcut"],
) {
  return (
    event.key === "Enter" &&
    !event.isComposing &&
    !event.shiftKey &&
    !event.altKey &&
    (shortcut === "enter"
      ? !event.metaKey && !event.ctrlKey
      : event.metaKey || event.ctrlKey)
  );
}

export function startupPage(
  storage: Pick<Storage, "getItem">,
): "workspace" | "main" | "home" {
  const preferences = parsePreferences(storage.getItem(PREFERENCES_KEY));
  if (preferences.startup === "home") return "home";
  // Preserve the existing startup behavior for users without preferences.
  const saved = storage.getItem("pipeline.ui.page");
  return saved === "workspace" || saved === "main" ? saved : "home";
}
