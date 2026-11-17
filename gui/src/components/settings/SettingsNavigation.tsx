import type { ReactNode } from "react";
import { Icon } from "../../ui/icons";
import ResizeHandle from "../ResizeHandle";
import usePersistentPanelWidth from "../../hooks/usePersistentPanelWidth";
import { type Section, SECTION_INFO } from "./navigation";
export function SettingsNavigation({
  section,
  query,
  onQueryChange,
  selectSection,
  showBack,
  handleClose,
  status,
}: {
  section: Section;
  query: string;
  onQueryChange: (query: string) => void;
  selectSection: (section: Section) => void;
  showBack: boolean;
  handleClose: () => void;
  status: ReactNode;
}) {
  const [navWidth, setNavWidth] = usePersistentPanelWidth(
    "pipeline.ui.settingsNavWidth",
    224,
    192,
    320,
  );
  return (
    <div style={{ width: navWidth }} className="settings-sidebar">
      <h2 className="settings-sidebar-title">Settings</h2>
      <div className="settings-search-bar">
        <div className="settings-search-field">
          <Icon name="search" className="settings-search-icon" />
          <label className="sr-only" htmlFor="settings-search">
            Search settings
          </label>
          <input
            id="settings-search"
            className="ui-input"
            type="search"
            value={query}
            placeholder="Search settings…"
            onChange={(e) => onQueryChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") onQueryChange("");
            }}
          />
        </div>
      </div>
      <nav aria-label="Settings categories">
        {(
          [
            "general",
            "providers",
            "conversations",
            "workflow",
            "storage",
            "notifications",
            "advanced",
          ] as const
        ).map((id) => (
          <button
            key={id}
            type="button"
            onClick={() => selectSection(id)}
            aria-current={section === id ? "page" : undefined}
            aria-label={SECTION_INFO[id].title}
            className={`settings-nav-item ${section === id ? "is-active" : ""}`}
          >
            <span className="min-w-0 font-medium">
              {SECTION_INFO[id].title}
            </span>
          </button>
        ))}
      </nav>
      <div className="settings-save-status" aria-live="polite">
        {status}
      </div>
      {showBack && (
        <button
          onClick={handleClose}
          className="flex items-center gap-2 px-2.5 py-2 text-sm text-gray-500 hover:text-gray-700 dark:text-neutral-400 dark:hover:text-neutral-200 transition-colors"
        >
          <svg
            className="w-4 h-4"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={1.5}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M10.5 19.5L3 12m0 0l7.5-7.5M3 12h18"
            />
          </svg>
          Back
        </button>
      )}
      <ResizeHandle
        currentWidth={navWidth}
        defaultWidth={224}
        label="Resize settings navigation"
        min={192}
        max={320}
        onResize={setNavWidth}
      />
    </div>
  );
}
