import type { ReactNode } from "react";
import ResizeHandle from "../ResizeHandle";
import usePersistentPanelWidth from "../../hooks/usePersistentPanelWidth";
import { type Section, SECTION_INFO } from "./navigation";
export function SettingsNavigation({
  section,
  selectSection,
  showBack,
  handleClose,
  status,
}: {
  section: Section;
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
      <nav aria-label="Settings categories" className="space-y-1 flex-1">
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
            <svg
              aria-hidden="true"
              className="h-4 w-4 shrink-0"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={1.5}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d={SECTION_INFO[id].icon}
              />
            </svg>
            <span className="min-w-0">
              <span className="block font-medium">
                {SECTION_INFO[id].title}
              </span>
              <span className="settings-nav-detail">
                {SECTION_INFO[id].detail}
              </span>
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
