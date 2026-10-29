// First-run guided setup: a device-local record of whether the researcher has
// closed the Get started sheet and which kind of work they chose. The choice
// only tailors tips; it is never copied into Settings or a Review.

export const GET_STARTED_KEY = "pipeline.ui.getStarted.v1";

export const GET_STARTED_PERSONAS = [
  "stata-latex",
  "r-word",
  "python-notebooks",
  "word-zotero",
  "other",
] as const;

export type GetStartedPersona = (typeof GET_STARTED_PERSONAS)[number];

export interface GetStartedRecord {
  version: 1;
  /** When the sheet was last closed; null while it has never been closed. */
  dismissedAt: string | null;
  persona: GetStartedPersona | null;
}

export type GetStartedView = "setup" | "example";

const EMPTY: GetStartedRecord = {
  version: 1,
  dismissedAt: null,
  persona: null,
};

function isPersona(value: unknown): value is GetStartedPersona {
  return (GET_STARTED_PERSONAS as readonly unknown[]).includes(value);
}

export function parseGetStarted(raw: string | null): GetStartedRecord | null {
  if (raw === null) return null;
  try {
    const value: unknown = JSON.parse(raw);
    if (!value || typeof value !== "object" || Array.isArray(value))
      return { ...EMPTY };
    const record = value as Record<string, unknown>;
    return {
      version: 1,
      dismissedAt:
        typeof record.dismissedAt === "string" && record.dismissedAt
          ? record.dismissedAt
          : null,
      persona: isPersona(record.persona) ? record.persona : null,
    };
  } catch {
    // A corrupt record still means the sheet was seen before.
    return { ...EMPTY };
  }
}

export function readGetStarted(
  storage: Pick<Storage, "getItem"> = localStorage,
): GetStartedRecord | null {
  try {
    return parseGetStarted(storage.getItem(GET_STARTED_KEY));
  } catch {
    return null;
  }
}

/** Best effort: a failed write only means the sheet may be offered again. */
export function saveGetStarted(
  patch: Partial<Omit<GetStartedRecord, "version">>,
  storage: Pick<Storage, "getItem" | "setItem"> = localStorage,
): GetStartedRecord {
  const next: GetStartedRecord = {
    ...(readGetStarted(storage) ?? EMPTY),
    ...patch,
    version: 1,
  };
  try {
    storage.setItem(GET_STARTED_KEY, JSON.stringify(next));
  } catch {
    /* Read-only storage. */
  }
  return next;
}

/**
 * Whether this device already has Pipeline interface state, which means the
 * app ran here before this sheet existed. Upgrades then reach the sheet from
 * Home or Settings instead of having it open over their work.
 */
export function hasPriorPipelineState(
  storage: Pick<Storage, "length" | "key">,
): boolean {
  for (let index = 0; index < storage.length; index += 1) {
    const key = storage.key(index);
    if (key?.startsWith("pipeline.") && key !== GET_STARTED_KEY) return true;
  }
  return false;
}

/** Offer the sheet once, on a first launch, until it has been closed. */
export function shouldOfferGetStarted(
  storage: Pick<Storage, "getItem" | "length" | "key"> = localStorage,
): boolean {
  try {
    if (readGetStarted(storage)?.dismissedAt) return false;
    return !hasPriorPipelineState(storage);
  } catch {
    return false;
  }
}

// Open requests from Home and Settings. Requests carry no state of their own,
// so a closed or unmounted host never leaves a sheet pending.
const listeners = new Set<(view: GetStartedView) => void>();

export function openGetStarted(view: GetStartedView = "setup") {
  for (const listener of [...listeners]) listener(view);
}

export function onGetStartedRequest(
  listener: (view: GetStartedView) => void,
): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
