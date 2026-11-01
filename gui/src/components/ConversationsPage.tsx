import { useState } from "react";
import useConversationIndex from "../hooks/useConversationIndex";
import { projectDate } from "../lib/projectIndex";
import PageHeader from "../ui/PageHeader";
import "./project-surface/ProjectHome.css";

type Filter = "all" | "unfiled" | "archived";

const FILTERS: Array<[Filter, string]> = [
  ["all", "All"],
  ["unfiled", "Unfiled"],
  ["archived", "Archived"],
];

/** Every conversation, with or without a project. */
export default function ConversationsPage({
  onOpen,
  onNew,
}: {
  onOpen: (sessionId: string) => Promise<unknown>;
  onNew: () => void;
}) {
  const [filter, setFilter] = useState<Filter>("all");
  const [search, setSearch] = useState("");
  const [openError, setOpenError] = useState<string | null>(null);
  const { conversations, loading, error, refresh } = useConversationIndex(
    true,
    filter === "archived",
    true,
  );
  const term = search.trim().toLocaleLowerCase();
  const visible = conversations.filter(({ session, projectName }) => {
    if (filter === "archived" && !session.archivedAt) return false;
    if (filter === "unfiled" && session.workspaceId) return false;
    return `${session.title} ${projectName ?? ""}`
      .toLocaleLowerCase()
      .includes(term);
  });
  const open = (id: string) => {
    setOpenError(null);
    onOpen(id).catch(() =>
      setOpenError("The conversation could not be opened. Please try again."),
    );
  };
  return (
    <div className="project-home project-index-page">
      <div className="ui-page">
        <PageHeader
          title="Conversations"
          description="Everything you have asked, with or without a project."
          action={
            <button
              type="button"
              className="ui-page-button primary"
              onClick={onNew}
            >
              + New conversation
            </button>
          }
        />
        <div className="ui-page-toolbar project-index-search">
          <label className="sr-only" htmlFor="conversation-search">
            Search conversations
          </label>
          <input
            id="conversation-search"
            type="search"
            placeholder="Search conversations…"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
          <div role="group" aria-label="Show" className="flex gap-1">
            {FILTERS.map(([value, label]) => (
              <button
                key={value}
                type="button"
                className={`ui-page-button ${filter === value ? "primary" : ""}`}
                aria-pressed={filter === value}
                onClick={() => setFilter(value)}
              >
                {label}
              </button>
            ))}
          </div>
          <button
            type="button"
            className="ui-page-button"
            disabled={loading}
            onClick={refresh}
          >
            Refresh
          </button>
        </div>
        {(error || openError) && (
          <p role="alert" className="project-home-error">
            {openError ?? error}
          </p>
        )}
        {loading && !conversations.length && (
          <p role="status" className="project-home-empty">
            Loading conversations…
          </p>
        )}
        {!loading && !error && !visible.length && (
          <p role="status" className="project-home-empty">
            {term
              ? `No conversations match “${search}”.`
              : filter === "archived"
                ? "No archived conversations."
                : "No conversations yet."}
          </p>
        )}
        {visible.length > 0 && (
          <section className="project-index-group" aria-label="Conversations">
            {visible.map(({ session, projectName }) => (
              <article
                className="ui-page-row project-index-row"
                key={session.id}
              >
                <div className="project-index-info">
                  <button
                    type="button"
                    className="project-index-name"
                    onClick={() => open(session.id)}
                  >
                    {session.title}
                  </button>
                  <p className="project-home-meta">
                    {projectName ?? "No project"} ·{" "}
                    {projectDate(session.updatedAt)}
                    {session.archivedAt ? " · Archived" : ""}
                  </p>
                </div>
              </article>
            ))}
          </section>
        )}
      </div>
    </div>
  );
}
