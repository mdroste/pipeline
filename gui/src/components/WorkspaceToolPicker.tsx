import { useEffect, useId, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import useModalDialog from "../hooks/useModalDialog";
import {
  loadWorkspacePins,
  workspaceDestinations,
  workspaceSections,
  type WorkspaceDestination,
} from "../lib/workspaceNavigation";
import { router } from "../lib/router";

// App-level verbs surfaced beside project views, so the palette can start
// work, not only navigate. Each runs through the guarded router.
const PICKER_ACTIONS = [
  {
    id: "action:review",
    label: "Start a review…",
    hint: "Reviews",
    run: () => void router.navigate({ page: "main" }),
  },
  {
    id: "action:automation",
    label: "New automation…",
    hint: "Automations",
    run: () => void router.navigate({ page: "tasks" }),
  },
] as const;
type PickerEntry = WorkspaceDestination | (typeof PICKER_ACTIONS)[number]["id"];
const isAction = (
  id: PickerEntry,
): id is (typeof PICKER_ACTIONS)[number]["id"] => id.startsWith("action:");
const entryLabel = (id: PickerEntry) =>
  isAction(id)
    ? PICKER_ACTIONS.find((action) => action.id === id)!.label
    : workspaceDestinations[id].label;
const entryHint = (id: PickerEntry) =>
  isAction(id)
    ? PICKER_ACTIONS.find((action) => action.id === id)!.hint
    : workspaceSections.find(
        (section) => section.id === workspaceDestinations[id].section,
      )!.label;

export default function WorkspaceToolPicker({
  workspaceId,
  current,
  onChoose,
  onClose,
}: {
  workspaceId: string;
  current: WorkspaceDestination;
  onChoose: (tool: WorkspaceDestination) => void;
  onClose: () => void;
}) {
  const root = useModalDialog<HTMLDivElement>(onClose, true, true);
  const title = useId();
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const pins = useMemo(() => loadWorkspacePins(workspaceId), [workspaceId]);
  const matches = useMemo<PickerEntry[]>(() => {
    const terms = query.toLowerCase().trim().split(/\s+/);
    const views = (Object.keys(workspaceDestinations) as WorkspaceDestination[])
      .filter((id) => {
        const item = workspaceDestinations[id];
        const section = workspaceSections.find(
          (section) => section.id === item.section,
        )!;
        return terms.every((term) =>
          `${id} ${item.label} ${section.label}`.toLowerCase().includes(term),
        );
      })
      .sort((a, b) => {
        const rank = (id: WorkspaceDestination) =>
          pins.includes(id) ? pins.indexOf(id) : pins.length;
        return rank(a) - rank(b);
      });
    const actions = PICKER_ACTIONS.filter((action) =>
      terms.every((term) =>
        `${action.label} ${action.hint}`.toLowerCase().includes(term),
      ),
    ).map((action) => action.id);
    return [...views, ...actions];
  }, [query, pins]);
  const selected = Math.min(index, matches.length - 1);
  useEffect(() => {
    root.current
      ?.querySelector<HTMLElement>('[data-highlighted="true"]')
      ?.scrollIntoView?.({ block: "nearest" });
  }, [selected]);
  const choose = (tool: PickerEntry) => {
    if (isAction(tool)) {
      PICKER_ACTIONS.find((action) => action.id === tool)!.run();
    } else {
      onChoose(tool);
    }
    onClose();
  };
  return createPortal(
    <div
      className="workspace-picker-backdrop"
      onPointerDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={root}
        role="dialog"
        aria-modal="true"
        aria-labelledby={title}
        className="workspace-tool-picker"
        tabIndex={-1}
      >
        <div className="workspace-picker-heading">
          <h2 id={title}>Open a project view</h2>
          <button
            type="button"
            aria-label="Close project view picker"
            onClick={onClose}
          >
            ×
          </button>
        </div>
        <input
          data-autofocus
          type="search"
          role="combobox"
          aria-expanded="true"
          aria-autocomplete="list"
          aria-label="Find a project view"
          placeholder="Search files, notes, results…"
          value={query}
          aria-controls={`${title}-results`}
          aria-activedescendant={
            selected >= 0 ? `${title}-${matches[selected]}` : undefined
          }
          onChange={(event) => {
            setQuery(event.target.value);
            setIndex(0);
          }}
          onKeyDown={(event) => {
            if (event.key === "ArrowDown" || event.key === "ArrowUp") {
              event.preventDefault();
              setIndex((value) =>
                Math.max(
                  0,
                  Math.min(
                    matches.length - 1,
                    value + (event.key === "ArrowDown" ? 1 : -1),
                  ),
                ),
              );
            }
            if (event.key === "Enter" && matches[selected]) {
              event.preventDefault();
              choose(matches[selected]);
            }
          }}
        />
        <div
          id={`${title}-results`}
          role="listbox"
          aria-label="Project views"
          className="workspace-picker-results"
        >
          {matches.map((id, row) => (
            <button
              type="button"
              role="option"
              aria-selected={row === selected}
              tabIndex={-1}
              id={`${title}-${id}`}
              key={id}
              data-highlighted={row === selected}
              aria-current={current === id ? "page" : undefined}
              onClick={() => choose(id)}
            >
              <span>{entryLabel(id)}</span>
              <small>{entryHint(id)}</small>
            </button>
          ))}
          {!matches.length && <p>No matching project views.</p>}
        </div>
        <footer>↑ ↓ to browse · Enter to open · Esc to close</footer>
      </div>
    </div>,
    document.body,
  );
}
