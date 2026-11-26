import { Icon } from "../../ui/icons";
import type { Dispatch, RefObject, SetStateAction } from "react";
import ResizeHandle from "../ResizeHandle";
import type { ReportNavigationEntry } from "../../lib/reportNavigation";
interface Props {
  compact: boolean;
  contents: ReportNavigationEntry[];
  contentsToggle: RefObject<HTMLButtonElement | null>;
  outlineVisible: boolean;
  outlineId: string;
  setCompactContentsOpen: Dispatch<SetStateAction<boolean>>;
  contentsWidth: number;
  hideContents: () => void;
  anchorPrefix: string;
  activeAnchor: string | null;
  setContentsWidth: (width: number) => void;
  contentsOpen: boolean;
  setContentsOpen: Dispatch<SetStateAction<boolean>>;
}
export default function ReportContents({
  compact,
  contents,
  contentsToggle,
  outlineVisible,
  outlineId,
  setCompactContentsOpen,
  contentsWidth,
  hideContents,
  anchorPrefix,
  activeAnchor,
  setContentsWidth,
  contentsOpen,
  setContentsOpen,
}: Props) {
  return (
    <>
      {/* Table of contents */}
      {compact && contents.length > 0 && (
        <button
          ref={contentsToggle}
          type="button"
          aria-expanded={outlineVisible}
          aria-controls={outlineId}
          onClick={() => setCompactContentsOpen((value) => !value)}
          className="reader-contents-toggle"
        >
          Contents <span aria-hidden="true">{outlineVisible ? "−" : "+"}</span>
        </button>
      )}
      {contents.length > 0 && outlineVisible && (
        <nav
          id={outlineId}
          aria-label="Table of contents"
          className={`toc-nav relative ${compact ? "toc-compact" : ""}`}
          style={{ width: compact ? "100%" : contentsWidth }}
        >
          <div className="mb-4 flex items-center justify-between gap-2">
            <h4 className="text-[0.6875rem] font-semibold uppercase tracking-widest text-gray-600 dark:text-gray-400">
              Contents
            </h4>
            <button
              type="button"
              onClick={hideContents}
              aria-label="Hide table of contents"
              className="rounded p-1 text-gray-500 hover:bg-gray-100 hover:text-gray-800
                         focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400
                         dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100"
            >
              <Icon name="chevron-right" className="h-3.5 w-3.5" />
            </button>
          </div>
          <ul className="space-y-0.5">
            {contents.map((entry) => (
              <li key={entry.id}>
                <a
                  href={`#${anchorPrefix}${entry.id}`}
                  aria-current={
                    activeAnchor === anchorPrefix + entry.id
                      ? "location"
                      : undefined
                  }
                  className={`toc-link ${
                    entry.kind === "issue"
                      ? "toc-issue"
                      : entry.level === 1
                        ? "toc-h1"
                        : "toc-h2"
                  }`}
                >
                  {entry.kind === "issue" && (
                    <span className="toc-issue-number">#{entry.number}</span>
                  )}
                  <span>{entry.text}</span>
                </a>
              </li>
            ))}
          </ul>
          {!compact && (
            <ResizeHandle
              minRemaining={420}
              currentWidth={contentsWidth}
              defaultWidth={240}
              label="Resize table of contents"
              min={184}
              max={360}
              onResize={setContentsWidth}
            />
          )}
        </nav>
      )}
      {contents.length > 0 && !outlineVisible && (
        <nav id={outlineId} aria-label="Table of contents" hidden />
      )}
      {contents.length > 0 && !compact && !contentsOpen && (
        <button
          type="button"
          onClick={() => setContentsOpen(true)}
          ref={contentsToggle}
          aria-label="Show table of contents"
          aria-expanded={false}
          aria-controls={outlineId}
          className="flex w-10 shrink-0 items-start justify-center border-r border-gray-200 pt-5 text-gray-500
                     hover:bg-gray-50 hover:text-gray-800 focus-visible:outline-none focus-visible:ring-2
                     focus-visible:ring-inset focus-visible:ring-gray-400 dark:border-gray-800 dark:hover:bg-gray-900
                     dark:text-gray-400 dark:hover:text-gray-100"
        >
          <Icon name="notes" className="h-4 w-4" />
        </button>
      )}
    </>
  );
}
