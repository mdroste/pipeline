import { isWorkspaceDestination } from "./workspaceNavigation";
import type { OpenResearchObject } from './deskClient';

export interface DeskLayout {
  tab: string;
  revisionId: string | null;
  pinnedRevision: string | null;
  object: OpenResearchObject | null;
  comparison: OpenResearchObject | null;
  assistantWidth: number;
}
const defaults: DeskLayout = { tab: 'overview', revisionId: null, pinnedRevision: null, object: null, comparison: null, assistantWidth: 40 };
const stringOrNull = (value: unknown) => typeof value === 'string' && value.length <= 500 ? value : null;
function reference(value: unknown): OpenResearchObject | null {
  if (!value || typeof value !== 'object') return null;
  const v = value as Record<string, unknown>;
  if (!stringOrNull(v.kind) || !stringOrNull(v.id) || !stringOrNull(v.revision)) return null;
  const offset = (n: unknown) => typeof n === 'number' && Number.isSafeInteger(n) && n >= 0 ? n : undefined;
  return { kind: v.kind as string, id: v.id as string, revision: v.revision as string, start: offset(v.start), end: offset(v.end) };
}
export function loadDeskLayout(workspaceId: string): DeskLayout {
  try {
    const v = JSON.parse(localStorage.getItem(`pipeline.desk.${workspaceId}`) ?? '{}') as Record<string, unknown> | null;
    if (!v || typeof v !== 'object') return { ...defaults };
    return {
      tab: isWorkspaceDestination(v.tab) ? v.tab : 'overview',
      revisionId: stringOrNull(v.revisionId), pinnedRevision: stringOrNull(v.pinnedRevision),
      object: reference(v.object), comparison: reference(v.comparison),
      assistantWidth: typeof v.assistantWidth === 'number' && Number.isFinite(v.assistantWidth) ? Math.max(30, Math.min(60, v.assistantWidth)) : 40,
    };
  } catch { return { ...defaults }; }
}
export function saveDeskLayout(workspaceId: string, value: DeskLayout) {
  try { localStorage.setItem(`pipeline.desk.${workspaceId}`, JSON.stringify(value)); }
  catch { /* Optional layout storage never grants model access. */ }
}
