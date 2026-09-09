import { useMemo, useState } from "react";
import SidebarPanel, { SidebarHeader } from "./SidebarPanel";
import WorkspaceIcon from "./WorkspaceIcon";
import usePersistentPanelWidth from "../hooks/usePersistentPanelWidth";

export interface ConversationEntry {
  id: string;
  text: string;
  role: "You" | "ChatGPT";
}
const PAGE_SIZE = 100;

export default function WorkspaceConversationOutline({
  entries,
  selectedId,
  onJump,
  onClose,
  embedded = false,
}: {
  entries: ConversationEntry[];
  selectedId: string | null;
  onJump: (id: string) => void;
  onClose: () => void;
  embedded?: boolean;
}) {
  const [width, setWidth] = usePersistentPanelWidth(
    "pipeline.workspace.outlineWidth",
    288,
    240,
    440,
  );
  const [query, setQuery] = useState("");
  const [role, setRole] = useState("all");
  const [page, setPage] = useState(0);
  const matches = useMemo(
    () =>
      entries
        .map((entry, index) => ({ ...entry, number: index + 1 }))
        .filter(
          (entry) =>
            (role === "all" || entry.role === role) &&
            entry.text
              .toLocaleLowerCase()
              .includes(query.trim().toLocaleLowerCase()),
        ),
    [entries, query, role],
  );
  const lastPage = Math.max(0, Math.ceil(matches.length / PAGE_SIZE) - 1);
  const currentPage = Math.min(page, lastPage);
  return (
    <SidebarPanel
      fill={embedded}
      aria-label="Conversation outline"
      side="right"
      width={width}
      defaultWidth={288}
      min={240}
      max={440}
      onResize={setWidth}
      resizeLabel="Resize conversation outline"
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          onClose();
        }
      }}
    >
      <SidebarHeader
        title="Outline"
        actions={
          <button
            type="button"
            aria-label="Close conversation outline"
            onClick={onClose}
            className="workspace-sidebar-icon-button"
          >
            <WorkspaceIcon name="close" />
          </button>
        }
      />
      <div className="space-y-2 px-5 pb-4">
        <input
          autoFocus
          type="search"
          aria-label="Search prompts and responses"
          placeholder="Find a prompt or response…"
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
            setPage(0);
          }}
          className="w-full rounded-lg border bg-white px-3 py-2 text-xs dark:border-neutral-700 dark:bg-neutral-950"
        />
        <select
          aria-label="Outline message type"
          value={role}
          onChange={(event) => {
            setRole(event.target.value);
            setPage(0);
          }}
          className="w-full rounded border bg-transparent px-2 py-1 text-xs"
        >
          <option value="all">Prompts and responses</option>
          <option value="You">Prompts only</option>
          <option value="ChatGPT">Responses only</option>
        </select>
        <p role="status" className="text-xs text-gray-500">
          {matches.length} {matches.length === 1 ? "message" : "messages"}
          {query ? " found" : ""}
        </p>
      </div>
      <nav
        aria-label="Prompts and responses"
        className="min-h-0 flex-1 overflow-auto px-3 pb-4"
      >
        {matches
          .slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE)
          .map((entry) => (
            <button
              type="button"
              key={entry.id}
              aria-current={selectedId === entry.id ? "location" : undefined}
              title={entry.text.slice(0, 400)}
              onClick={() => onJump(entry.id)}
              className={`mb-1 block w-full rounded-lg px-2 py-3 text-left transition-colors hover:bg-gray-50 dark:hover:bg-gray-800/60 ${selectedId === entry.id ? "bg-gray-100 dark:bg-gray-800" : ""}`}
            >
              <span className="mb-1 block text-[10px] font-semibold uppercase tracking-wide text-gray-500">
                {entry.number} · {entry.role === "You" ? "Prompt" : "Response"}
              </span>
              <span className="line-clamp-2 break-words text-xs">
                {entry.text.replace(/\s+/g, " ").slice(0, 220)}
              </span>
            </button>
          ))}
        {!matches.length && (
          <p className="p-3 text-xs text-gray-500">
            {entries.length
              ? "No matching messages."
              : "Prompts and responses will appear here."}
          </p>
        )}
      </nav>
      {lastPage > 0 && (
        <div className="flex items-center justify-between border-t p-3 text-xs dark:border-neutral-800">
          <button
            type="button"
            disabled={currentPage === 0}
            onClick={() => setPage(currentPage - 1)}
            className="rounded border px-2 py-1 disabled:opacity-40"
          >
            Previous
          </button>
          <span>
            {currentPage + 1} / {lastPage + 1}
          </span>
          <button
            type="button"
            disabled={currentPage === lastPage}
            onClick={() => setPage(currentPage + 1)}
            className="rounded border px-2 py-1 disabled:opacity-40"
          >
            Next
          </button>
        </div>
      )}
    </SidebarPanel>
  );
}
