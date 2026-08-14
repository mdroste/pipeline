import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import useModalDialog from "../hooks/useModalDialog";
import type {
  AutoReviewCatalog,
  AutoReviewCatalogGroup,
  AutoReviewCatalogRole,
} from "../lib/types";

interface Props {
  onClose: () => void;
  initialTab?: CatalogTab;
}

type CatalogTab = "subjects" | "methods";

function matches(role: AutoReviewCatalogRole, query: string): boolean {
  if (!query) return true;
  const searchable = `${role.label} ${role.description} ${role.exclusions}`.toLowerCase();
  return searchable.includes(query);
}

function shortRoleLabel(role: AutoReviewCatalogRole, group?: string): string {
  if (!group) return role.label.replace(/^(Method|Genre) — /, "");
  const prefix = `${group} — `;
  return role.label.startsWith(prefix)
    ? role.label.slice(prefix.length)
    : role.label.replace(/^(Method|Genre) — /, "");
}

function RoleCard({ role, group }: { role: AutoReviewCatalogRole; group?: string }) {
  const isFallback = role.level === "discipline" || role.level === "family";
  return (
    <article className="rounded-xl border border-gray-200 bg-white p-3 dark:border-gray-800 dark:bg-gray-900">
      <div className="flex items-start justify-between gap-3">
        <h3 className="text-sm font-medium text-gray-950 dark:text-gray-100">
          {shortRoleLabel(role, group)}
        </h3>
        {isFallback && (
          <span className="shrink-0 rounded-full bg-gray-100 px-2 py-0.5 text-[9px] font-semibold uppercase tracking-wide text-gray-500 dark:bg-gray-800 dark:text-gray-400">
            Broad fallback
          </span>
        )}
      </div>
      <p className="mt-1 text-xs leading-5 text-gray-600 dark:text-gray-300">{role.description}</p>
      <p className="mt-2 text-[11px] leading-4 text-gray-500 dark:text-gray-400">
        <span className="font-medium text-gray-600 dark:text-gray-300">Not selected for:</span>{" "}
        {role.exclusions}
      </p>
    </article>
  );
}

function GroupedTab({
  groups,
  navLabel,
  selectedGroup,
  onSelectGroup,
  hint,
}: {
  groups: AutoReviewCatalogGroup[];
  navLabel: string;
  selectedGroup: string;
  onSelectGroup: (id: string) => void;
  hint: string;
}) {
  const active = groups.find((group) => group.id === selectedGroup) ?? groups[0];
  if (!active) return null;
  return (
    <div className="grid gap-4 md:grid-cols-[13rem_minmax(0,1fr)]">
      <nav aria-label={navLabel} className="max-h-[58vh] overflow-auto rounded-xl border border-gray-200 bg-white p-1.5 dark:border-gray-800 dark:bg-gray-900">
        {groups.map((group) => (
          <button
            key={group.id}
            type="button"
            onClick={() => onSelectGroup(group.id)}
            className={`flex w-full items-center justify-between rounded-lg px-2.5 py-2 text-left text-xs ${group.id === active.id ? "bg-blue-50 font-medium text-blue-900 dark:bg-blue-950/50 dark:text-blue-100" : "text-gray-600 hover:bg-gray-50 dark:text-gray-300 dark:hover:bg-gray-800"}`}
          >
            <span>{group.label}</span>
            <span className="ml-2 text-[10px] text-gray-400">{group.roles.length}</span>
          </button>
        ))}
      </nav>
      <section aria-label={active.label}>
        <div className="mb-3">
          <h3 className="text-base font-semibold text-gray-950 dark:text-gray-100">{active.label}</h3>
          <p className="mt-0.5 text-[11px] text-gray-500 dark:text-gray-400">{hint}</p>
        </div>
        <div className="grid gap-2 xl:grid-cols-2">
          {active.roles.map((role) => (
            <RoleCard key={role.id} role={role} group={active.label} />
          ))}
        </div>
      </section>
    </div>
  );
}

export default function AutoReviewCatalogDialog({ onClose, initialTab = "subjects" }: Props) {
  const dialogRef = useModalDialog<HTMLDivElement>(onClose);
  const [catalog, setCatalog] = useState<AutoReviewCatalog | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<CatalogTab>(initialTab);
  const [query, setQuery] = useState("");
  const [selectedDiscipline, setSelectedDiscipline] = useState("");
  const [selectedFamily, setSelectedFamily] = useState("");

  useEffect(() => {
    let stale = false;
    invoke<AutoReviewCatalog>("get_auto_review_catalog")
      .then((value) => {
        if (stale) return;
        setCatalog(value);
        setSelectedDiscipline(value.disciplines[0]?.id ?? "");
        setSelectedFamily(value.methodFamilies[0]?.id ?? "");
      })
      .catch((caught) => {
        if (!stale) setError(caught instanceof Error ? caught.message : String(caught));
      });
    return () => {
      stale = true;
    };
  }, []);

  const normalizedQuery = query.trim().toLowerCase();
  const filterGroups = (groups: AutoReviewCatalogGroup[]) => groups
    .map((group) => ({
      ...group,
      roles: group.roles.filter((role) => matches(role, normalizedQuery)),
    }))
    .filter((group) => group.roles.length > 0);
  const matchingDisciplines = useMemo(
    () => (catalog && normalizedQuery ? filterGroups(catalog.disciplines) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [catalog, normalizedQuery],
  );
  const matchingFamilies = useMemo(
    () => (catalog && normalizedQuery ? filterGroups(catalog.methodFamilies) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [catalog, normalizedQuery],
  );

  const tabButton = (id: CatalogTab, text: string) => (
    <button
      type="button"
      role="tab"
      aria-selected={tab === id}
      onClick={() => setTab(id)}
      className={`rounded-md px-3 py-1.5 text-xs font-medium ${tab === id ? "bg-white text-gray-950 shadow-sm dark:bg-gray-700 dark:text-white" : "text-gray-500 dark:text-gray-400"}`}
    >
      {text}
    </button>
  );

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-4 sm:p-6"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="auto-review-catalog-title"
        tabIndex={-1}
        className="flex max-h-[90vh] w-full max-w-5xl flex-col overflow-hidden rounded-2xl bg-gray-50 shadow-2xl dark:bg-gray-950"
      >
        <header className="border-b border-gray-200 bg-white px-5 py-4 dark:border-gray-800 dark:bg-gray-900 sm:px-6">
          <div className="flex items-start gap-4">
            <div className="min-w-0 flex-1">
              <p className="text-[10px] font-semibold uppercase tracking-[0.14em] text-blue-700 dark:text-blue-300">
                Auto Paper Review
              </p>
              <h2 id="auto-review-catalog-title" className="mt-1 text-xl font-semibold text-gray-950 dark:text-gray-50">
                Specialist catalog
              </h2>
              <p className="mt-1 max-w-3xl text-xs leading-5 text-gray-500 dark:text-gray-400">
                The orientation pass selects one primary subject specialist, an optional second subject specialist for genuinely interdisciplinary work, and one to four method specialists. Only those reviewers are assembled for the report; it also classifies the document's genre, which every reviewer receives as shared context.
              </p>
            </div>
            <button
              type="button"
              onClick={onClose}
              aria-label="Close specialist catalog"
              className="rounded-lg p-1.5 text-gray-400 hover:bg-gray-100 hover:text-gray-800 dark:hover:bg-gray-800 dark:hover:text-gray-100"
            >
              ✕
            </button>
          </div>
          {catalog && (
            <div className="mt-3 flex flex-wrap gap-2 text-[11px] text-gray-600 dark:text-gray-300">
              <span className="rounded-full bg-blue-50 px-2.5 py-1 dark:bg-blue-950/40">
                {catalog.disciplines.length} discipline{catalog.disciplines.length === 1 ? "" : "s"}
              </span>
              <span className="rounded-full bg-blue-50 px-2.5 py-1 dark:bg-blue-950/40">{catalog.subjectCount} subject roles</span>
              <span className="rounded-full bg-blue-50 px-2.5 py-1 dark:bg-blue-950/40">
                {catalog.methodCount} method roles in {catalog.methodFamilies.length} families
              </span>
            </div>
          )}
        </header>

        <div className="border-b border-gray-200 bg-white px-5 py-3 dark:border-gray-800 dark:bg-gray-900 sm:px-6">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <div className="inline-flex w-fit rounded-lg bg-gray-100 p-1 dark:bg-gray-800" role="tablist" aria-label="Specialist type">
              {tabButton("subjects", `Subjects${catalog ? ` (${catalog.subjectCount})` : ""}`)}
              {tabButton("methods", `Methods${catalog ? ` (${catalog.methodCount})` : ""}`)}
            </div>
            <label className="relative block w-full sm:max-w-xs">
              <span className="sr-only">Search specialists</span>
              <input
                data-autofocus
                type="search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Search roles and review focus…"
                className="w-full rounded-lg border border-gray-300 bg-white px-3 py-2 text-xs text-gray-900 outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-100 dark:border-gray-700 dark:bg-gray-950 dark:text-gray-100 dark:focus:ring-blue-950"
              />
            </label>
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-auto p-4 sm:p-5">
          {!catalog && !error && <p role="status" className="text-sm text-gray-500">Loading specialist catalog…</p>}
          {error && <p role="alert" className="text-sm text-red-700 dark:text-red-300">Could not load the specialist catalog: {error}</p>}

          {catalog && tab === "subjects" && !normalizedQuery && (
            <GroupedTab
              groups={catalog.disciplines}
              navLabel="Academic disciplines"
              selectedGroup={selectedDiscipline}
              onSelectGroup={setSelectedDiscipline}
              hint="The broad role is used only when no listed subfield fits."
            />
          )}
          {catalog && tab === "subjects" && normalizedQuery && (
            <SearchGroupResults groups={matchingDisciplines} />
          )}

          {catalog && tab === "methods" && !normalizedQuery && (
            <GroupedTab
              groups={catalog.methodFamilies}
              navLabel="Method families"
              selectedGroup={selectedFamily}
              onSelectGroup={setSelectedFamily}
              hint="Within a family, a specific role is preferred; the broad fallback covers unlisted or spanning approaches."
            />
          )}
          {catalog && tab === "methods" && normalizedQuery && (
            <SearchGroupResults groups={matchingFamilies} />
          )}
        </div>

        <footer className="flex items-center justify-between border-t border-gray-200 bg-white px-5 py-3 dark:border-gray-800 dark:bg-gray-900 sm:px-6">
          <p className="text-[10px] text-gray-500 dark:text-gray-400">Subject expertise and method scrutiny are complementary.</p>
          <button type="button" onClick={onClose} className="rounded-lg bg-gray-900 px-4 py-2 text-xs font-medium text-white dark:bg-gray-100 dark:text-gray-900">
            Done
          </button>
        </footer>
      </div>
    </div>
  );
}

function SearchGroupResults({ groups }: { groups: AutoReviewCatalogGroup[] }) {
  if (groups.length === 0) return <NoMatches />;
  return (
    <div className="space-y-5">
      {groups.map((group) => (
        <section key={group.id}>
          <h3 className="mb-2 text-sm font-semibold text-gray-950 dark:text-gray-100">{group.label}</h3>
          <div className="grid gap-2 md:grid-cols-2 xl:grid-cols-3">
            {group.roles.map((role) => (
              <RoleCard key={role.id} role={role} group={group.label} />
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}

function NoMatches() {
  return <p className="rounded-xl border border-dashed border-gray-300 p-8 text-center text-sm text-gray-500 dark:border-gray-700 dark:text-gray-400">No specialists match this search.</p>;
}
