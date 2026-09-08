import { memo, useMemo, useState } from "react";
import type { ArtifactEntry, RunManifest } from "../../lib/artifactTypes";
import ResizeHandle from "../ResizeHandle";
import usePersistentPanelWidth from "../../hooks/usePersistentPanelWidth";
import { formatBytes } from "./format";
import { GROUPS, groupAgentReports } from "./grouping";
import { PageBrowser } from "./PageNavigator";
const ArtifactList = memo(function ArtifactList({
  items,
  onSelect,
  selected,
}: {
  items: ArtifactEntry[];
  onSelect: (path: string) => void;
  selected: string;
}) {
  const [expanded, setExpanded] = useState(false);
  const visible = expanded ? items : items.slice(0, 10);

  return (
    <>
      <ul className="space-y-0.5">
        {visible.map((item) => (
          <li key={item.rel_path}>
            <button
              onClick={() => onSelect(item.rel_path)}
              title={`${item.rel_path} · ${formatBytes(item.bytes)}`}
              className={`w-full truncate rounded px-2 py-1 text-left text-xs transition-colors ${
                selected === item.rel_path
                  ? "bg-gray-900 text-white dark:bg-gray-100 dark:text-gray-900"
                  : "text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800"
              }`}
            >
              {item.label}
            </button>
          </li>
        ))}
      </ul>
      {items.length > 10 && (
        <button
          type="button"
          onClick={() => setExpanded((value) => !value)}
          className="mt-1 w-full rounded px-2 py-1 text-left text-[11px] text-gray-600 hover:bg-gray-100
                     hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100"
        >
          {expanded ? "Show fewer" : `Show ${items.length - 10} more`}
        </button>
      )}
    </>
  );
});

const AgentReportList = memo(function AgentReportList({
  items,
  stepOutputs,
  onSelect,
  selected,
}: {
  items: ArtifactEntry[];
  stepOutputs: ArtifactEntry[];
  onSelect: (path: string) => void;
  selected: string;
}) {
  const groups = useMemo(
    () => groupAgentReports(items, stepOutputs),
    [items, stepOutputs],
  );

  return (
    <div className="space-y-2.5">
      {groups.map((group) => (
        <section key={group.key}>
          <h5
            className="mb-1 truncate px-2 text-[11px] font-medium text-gray-700 dark:text-gray-300"
            title={group.label}
          >
            {group.label}
          </h5>
          <ArtifactList
            items={group.items}
            selected={selected}
            onSelect={onSelect}
          />
        </section>
      ))}
    </div>
  );
});

export default function ArtifactTree({
  navigationOpen,
  setNavigationOpen,
  manifest,
  artifactsByGroup,
  selected,
  selectArtifact,
}: {
  navigationOpen: boolean;
  setNavigationOpen: (open: boolean) => void;
  manifest: RunManifest;
  artifactsByGroup: Map<string, ArtifactEntry[]>;
  selected: string;
  selectArtifact: (path: string) => void;
}) {
  const [treeWidth, setTreeWidth] = usePersistentPanelWidth(
    "pipeline.ui.artifactTreeWidth",
    224,
    176,
    360,
  );
  return (
    <>
      {navigationOpen ? (
        <nav
          style={{ width: treeWidth, maxWidth: "42vw" }}
          className="relative shrink-0 border-r border-gray-200 dark:border-gray-800"
        >
          <div className="h-full space-y-4 overflow-y-auto p-3">
            <div className="flex items-center justify-between gap-2 px-1">
              <span className="text-[11px] font-semibold uppercase tracking-[0.12em] text-gray-600 dark:text-gray-400">
                Artifacts
              </span>
              <button
                type="button"
                onClick={() => setNavigationOpen(false)}
                aria-label="Hide artifact browser"
                className="rounded p-1 text-gray-500 hover:bg-gray-100 hover:text-gray-800
                           focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400
                           dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100"
              >
                <svg
                  aria-hidden="true"
                  className="h-3.5 w-3.5"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth={1.8}
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="m9 6 6 6-6 6"
                  />
                </svg>
              </button>
            </div>
            {GROUPS.map((g) => {
              const items = artifactsByGroup.get(g.id) ?? [];
              const compactPages =
                g.id === "pages" ? manifest.page_artifacts : null;
              if (items.length === 0 && !compactPages?.count) return null;
              return (
                <div key={g.id}>
                  <h4 className="mb-1.5 text-[10px] font-semibold uppercase tracking-widest text-gray-600 dark:text-gray-400">
                    {g.label}
                  </h4>
                  {g.description && (
                    <p className="mb-1.5 px-0.5 text-[10px] leading-snug text-gray-500 dark:text-gray-500">
                      {g.description}
                    </p>
                  )}
                  {g.id === "pages" ? (
                    <PageBrowser
                      items={items}
                      index={compactPages}
                      selected={selected}
                      onSelect={selectArtifact}
                    />
                  ) : g.id === "agent_response" ? (
                    <AgentReportList
                      items={items}
                      stepOutputs={artifactsByGroup.get("step") ?? []}
                      selected={selected}
                      onSelect={selectArtifact}
                    />
                  ) : (
                    <ArtifactList
                      items={items}
                      selected={selected}
                      onSelect={selectArtifact}
                    />
                  )}
                </div>
              );
            })}
          </div>
          <ResizeHandle
            currentWidth={treeWidth}
            defaultWidth={224}
            label="Resize artifact browser"
            min={176}
            max={360}
            onResize={setTreeWidth}
          />
        </nav>
      ) : (
        <button
          type="button"
          onClick={() => setNavigationOpen(true)}
          aria-label="Show artifact browser"
          title="Show artifacts"
          className="flex w-10 shrink-0 items-start justify-center border-r border-gray-200 pt-4 text-gray-500
                     hover:bg-gray-50 hover:text-gray-800 focus-visible:outline-none focus-visible:ring-2
                     focus-visible:ring-inset focus-visible:ring-gray-400 dark:border-gray-800 dark:hover:bg-gray-900
                     dark:text-gray-400 dark:hover:text-gray-100"
        >
          <svg
            aria-hidden="true"
            className="h-4 w-4"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={1.7}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M7 4.5h10M7 9.5h10M7 14.5h6M4 4.5h.01M4 9.5h.01M4 14.5h.01"
            />
          </svg>
        </button>
      )}
    </>
  );
}
