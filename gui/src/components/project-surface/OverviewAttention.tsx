import { useRef, useState } from "react";
import type {
  OverviewAction,
  OverviewRow,
} from "../../lib/project-overview/types";
import {
  attentionVersion,
  deferred,
  orderAttention,
  priorityPinned,
  type AttentionPreferences,
} from "../../lib/project-overview/attention";
import { OverviewRowView } from "./OverviewRows";
import WorkspaceMenu from "../WorkspaceMenu";
import { MenuItem } from "../../ui/Menu";

export default function OverviewAttention({
  rows,
  preferences,
  onPreferences,
  onAction,
  preview = false,
}: {
  rows: OverviewRow[];
  preferences: AttentionPreferences;
  onPreferences: (value: AttentionPreferences) => void;
  onAction: (action: OverviewAction) => void;
  preview?: boolean;
}) {
  const [query, setQuery] = useState("");
  const [later, setLater] = useState(false);
  const [focused, setFocused] = useState(false);
  const order = useRef<string[]>([]);
  const ranked = orderAttention(rows, preferences);
  if (!focused) order.current = ranked.map((row) => row.id);
  const stable = focused
    ? [...ranked].sort((a, b) => {
        const position = (id: string) => {
          const i = order.current.indexOf(id);
          return i < 0 ? Infinity : i;
        };
        return position(a.id) - position(b.id);
      })
    : ranked;
  const visible = stable.filter(
    (row) =>
      (later || !deferred(row, preferences)) &&
      `${row.text} ${row.detail ?? ""}`
        .toLocaleLowerCase()
        .includes(query.toLocaleLowerCase()),
  );
  return (
    <div
      className="overview-attention-list"
      onFocusCapture={() => setFocused(true)}
      onBlurCapture={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null))
          setFocused(false);
      }}
    >
      {!preview && (
        <div className="overview-actions">
          <input
            className="ui-input"
            type="search"
            aria-label="Find attention items"
            placeholder="Find an item…"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
          <label>
            <input
              type="checkbox"
              checked={later}
              onChange={(event) => setLater(event.target.checked)}
            />{" "}
            Include later
          </label>
        </div>
      )}
      {(preview ? visible.slice(0, 3) : visible).map((row) => (
        <div className="overview-attention-item" key={row.id}>
          <OverviewRowView row={row} onAction={onAction} />
          {priorityPinned(row, preferences) && (
            <span className="overview-meta">Pinned</span>
          )}
          <WorkspaceMenu label={`Priority for ${row.text}`}>
            {row.category !== "blocker" && (
              <MenuItem
                onSelect={() =>
                  onPreferences({
                    ...preferences,
                    [row.id]: {
                      version: attentionVersion(row),
                      pinned: !priorityPinned(row, preferences),
                      pinnedUntil: new Date().setHours(24, 0, 0, 0),
                    },
                  })
                }
              >
                {priorityPinned(row, preferences)
                  ? "Unpin priority"
                  : "Pin for today"}
              </MenuItem>
            )}
            {row.category !== "blocker" && (
              <MenuItem
                onSelect={() =>
                  onPreferences({
                    ...preferences,
                    [row.id]: {
                      version: attentionVersion(row),
                      laterUntil: deferred(row, preferences)
                        ? 0
                        : Date.now() + 86_400_000,
                    },
                  })
                }
              >
                {deferred(row, preferences)
                  ? "Bring back now"
                  : "Later · 24 hours"}
              </MenuItem>
            )}
            {row.category === "blocker" && (
              <MenuItem disabled>Resolve this blocker to clear it.</MenuItem>
            )}
          </WorkspaceMenu>
        </div>
      ))}
      {!preview && !visible.length && (
        <p className="overview-meta">
          {later
            ? "No matching items."
            : "No matching items due now. Include later to see deferred work."}
        </p>
      )}
    </div>
  );
}
