import type { OpenResearchObject } from '../../lib/deskClient';
export interface DeskProps {workspaceId:string;sessionId?:string|null;onOpen:(object:OpenResearchObject)=>void;onError:(message:string)=>void;onRefresh?:()=>Promise<void>}
export const button='rounded border px-3 py-1.5 text-xs disabled:opacity-40';
export const input='w-full rounded border bg-transparent px-3 py-2 text-sm';
export const card='space-y-3 rounded-xl border bg-white p-4 dark:bg-neutral-950';
export const muted='text-xs text-gray-500';
