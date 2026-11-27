// Open conversations as tabs, styled and dragged like open file tabs. Drag a
// tab along the strip to reorder; drag it off the strip to move the whole
// conversation pane to the left, right or bottom of the project.

import { useRef } from "react";
import useTabList from "../../hooks/useTabList";
import useTabReorder from "../file-workspace/useTabReorder";
import type { ConversationTab } from "../../hooks/workspace/useConversationTabs";
import { useDeskDock } from "../deskDock";
import { Icon } from "../../ui/icons";
import "../file-workspace/files.css";
import "./ConversationTabs.css";

export default function ConversationTabs({
  tabs,
  active,
  working,
  disabled,
  onSelect,
  onClose,
  onMove,
}: {
  tabs: readonly ConversationTab[];
  active: string | null;
  /** The active conversation is replying. */
  working: boolean;
  /** Switching is unavailable, as while context is being changed. */
  disabled: boolean;
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
  onMove: (id: string, index: number) => void;
}) {
  const strip = useRef<HTMLDivElement>(null);
  const ids = tabs.map((tab) => tab.id);
  const list = useTabList(ids, active ?? "", onSelect, "manual");
  const desk = useDeskDock();
  const reorder = useTabReorder(
    strip,
    ids,
    onMove,
    desk
      ? {
          move: (_, x, y) => desk.move(x, y),
          end: (_, drop) => desk.end(drop),
        }
      : undefined,
    (id) => tabs.find((tab) => tab.id === id)?.title ?? id,
  );
  const hint = desk
    ? "Drag to reorder, or off the tabs to move the conversation"
    : "Drag to reorder";
  return (
    <div
      ref={strip}
      className="file-tabs workspace-conversation-tabs"
      role="tablist"
      aria-label="Open conversations"
    >
      {tabs.map((tab, position) => {
        const props = list.tabProps(tab.id);
        const selected = tab.id === active;
        return (
          <div
            key={tab.id}
            className="file-tab"
            role="presentation"
            data-active={selected}
          >
            <button
              {...props}
              // The conversation below is the panel for whichever tab is
              // selected; it is not a separate element per tab.
              aria-controls={undefined}
              disabled={disabled && !selected}
              className="file-tab-name"
              title={`${tab.title} · ${hint} · Delete to close`}
              aria-keyshortcuts="Control+Shift+PageUp Control+Shift+PageDown Delete"
              onPointerDown={(event) => reorder.onPointerDown(tab.id, event)}
              onKeyDown={(event) => {
                const step =
                  (event.ctrlKey || event.metaKey) && event.shiftKey
                    ? { PageUp: -1, PageDown: 1 }[event.key]
                    : undefined;
                if (event.key === "Delete") {
                  event.preventDefault();
                  onClose(tab.id);
                } else if (step) {
                  event.preventDefault();
                  onMove(tab.id, position + step);
                  requestAnimationFrame(() => list.focus(tab.id));
                } else props.onKeyDown(event);
              }}
            >
              {selected && working && (
                <span className="workspace-tab-working" aria-hidden="true" />
              )}
              <span className="file-tab-label">{tab.title}</span>
            </button>
            <button
              type="button"
              className="file-tab-close"
              tabIndex={selected ? 0 : -1}
              aria-label={`Close ${tab.title}`}
              disabled={disabled}
              onClick={() => onClose(tab.id)}
            >
              <Icon name="close" className="h-3 w-3" />
            </button>
          </div>
        );
      })}
    </div>
  );
}
