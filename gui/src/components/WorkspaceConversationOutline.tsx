import { useMemo, useState } from "react";

export interface ConversationEntry { id: string; text: string; role: "You" | "ChatGPT"; }
const PAGE_SIZE = 100;

export default function WorkspaceConversationOutline({ entries, selectedId, onJump, onClose }: {
  entries: ConversationEntry[]; selectedId: string | null; onJump: (id: string) => void; onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [role, setRole] = useState("all");
  const [page, setPage] = useState(0);
  const matches = useMemo(() => entries.map((entry, index) => ({ ...entry, number: index + 1 })).filter(entry =>
    (role === "all" || entry.role === role) && entry.text.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()),
  ), [entries, query, role]);
  const lastPage = Math.max(0, Math.ceil(matches.length / PAGE_SIZE) - 1);
  const currentPage = Math.min(page, lastPage);
  return <aside aria-label="Conversation outline" onKeyDown={event => { if (event.key === "Escape") { event.stopPropagation(); onClose(); } }} className="flex w-64 shrink-0 flex-col border-l border-gray-200 bg-gray-50 dark:border-neutral-800 dark:bg-neutral-900">
    <div className="flex items-center justify-between p-4"><h2 className="text-sm font-semibold">Conversation outline</h2><button type="button" aria-label="Close conversation outline" onClick={onClose} className="rounded px-2 py-1 hover:bg-gray-200 dark:hover:bg-neutral-800">×</button></div>
    <div className="space-y-2 px-4 pb-3">
      <input autoFocus type="search" aria-label="Search prompts and responses" placeholder="Find a prompt or response…" value={query} onChange={event => { setQuery(event.target.value); setPage(0); }} className="w-full rounded-lg border bg-white px-3 py-2 text-xs dark:border-neutral-700 dark:bg-neutral-950" />
      <select aria-label="Outline message type" value={role} onChange={event => { setRole(event.target.value); setPage(0); }} className="w-full rounded border bg-transparent px-2 py-1 text-xs"><option value="all">Prompts and responses</option><option value="You">Prompts only</option><option value="ChatGPT">Responses only</option></select>
      <p role="status" className="text-xs text-gray-500">{matches.length} {matches.length === 1 ? "message" : "messages"}{query ? " found" : ""}</p>
    </div>
    <nav aria-label="Prompts and responses" className="min-h-0 flex-1 overflow-auto px-2 pb-3">
      {matches.slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE).map(entry => <button type="button" key={entry.id} aria-current={selectedId === entry.id ? "location" : undefined} title={entry.text.slice(0, 400)} onClick={() => onJump(entry.id)} className={`mb-1 block w-full rounded-lg p-3 text-left hover:bg-white dark:hover:bg-neutral-800 ${selectedId === entry.id ? "bg-white ring-1 ring-blue-400 dark:bg-neutral-800" : ""}`}>
        <span className="mb-1 block text-[10px] font-semibold uppercase tracking-wide text-gray-500">{entry.number} · {entry.role === "You" ? "Prompt" : "Response"}</span>
        <span className="line-clamp-2 break-words text-xs">{entry.text.replace(/\s+/g, " ").slice(0, 220)}</span>
      </button>)}
      {!matches.length && <p className="p-3 text-xs text-gray-500">{entries.length ? "No matching messages." : "Prompts and responses will appear here."}</p>}
    </nav>
    {lastPage > 0 && <div className="flex items-center justify-between border-t p-3 text-xs dark:border-neutral-800"><button type="button" disabled={currentPage === 0} onClick={() => setPage(currentPage - 1)} className="rounded border px-2 py-1 disabled:opacity-40">Previous</button><span>{currentPage + 1} / {lastPage + 1}</span><button type="button" disabled={currentPage === lastPage} onClick={() => setPage(currentPage + 1)} className="rounded border px-2 py-1 disabled:opacity-40">Next</button></div>}
  </aside>;
}
