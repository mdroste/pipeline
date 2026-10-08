import { useCallback, useEffect, useRef, useState } from "react";
import {
  deskClient,
  type ContextItem,
  type ContextSelection,
  type OpenResearchObject,
} from "../lib/deskClient";
import { appEvents } from "../lib/appEvents";
import { workbenchErrorMessage } from "../lib/workbenchError";
import useContainerWidth from "../hooks/useContainerWidth";
import { asContextItems } from "../lib/workspaceAttachments";
import Popover from "../ui/Popover";
import Tooltip from "../ui/Tooltip";
import { Menu, MenuItem, MenuLabel } from "../ui/Menu";
import { Icon, type IconName } from "../ui/icons";

export const addContextObject = (
  workspaceId: string,
  object: OpenResearchObject,
) => appEvents.emit("context-add", { workspaceId, objects: [object] });
/** Add several sources at once, as soon as the conversation can take them. */
export const addContextObjectsWhenReady = (
  workspaceId: string,
  objects: OpenResearchObject[],
) => appEvents.emit("context-add", { workspaceId, objects, whenReady: true });

const key = (o: OpenResearchObject) =>
  `${o.kind}:${o.id}:${o.revision}:${o.start ?? ""}:${o.end ?? ""}`;
/** How a source is used, in the words a researcher would use. */
export const sourceRoleLabels: Record<ContextItem["role"], string> = {
  main: "Main paper",
  source: "Source",
  data_dictionary: "Data dictionary",
  prior_draft: "Earlier draft",
  referee_report: "Referee report",
  result: "Result",
  supporting: "Supporting material",
};
const roles = Object.keys(sourceRoleLabels) as ContextItem["role"][];
const kindIcon = (kind: string): IconName =>
  kind === "dataset" ? "data" : kind === "result" ? "result" : "file";
/** Chips shown in the row before measuring; the rest sit behind "+N more". */
const DEFAULT_VISIBLE = 3;
/** Each visible chip keeps room for a readable name beside the overflow button. */
const visibleFor = (width: number | null) =>
  width === null
    ? DEFAULT_VISIBLE
    : Math.max(1, Math.min(4, Math.floor((width - 70) / 126)));

function SourceChip({
  name,
  item,
  disabled,
  onRole,
  onRemove,
}: {
  name: string;
  item: ContextItem;
  disabled: boolean;
  onRole: (role: ContextItem["role"]) => void;
  onRemove: () => void;
}) {
  const trigger = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const role = sourceRoleLabels[item.role] ?? item.role;
  return (
    <span className="workspace-source-chip">
      <Tooltip label={`${name} · ${role}`}>
        <button
          ref={trigger}
          type="button"
          aria-haspopup="menu"
          aria-expanded={open}
          aria-label={`${name}, used as ${role}. Change how it is used`}
          disabled={disabled}
          onClick={() => setOpen((value) => !value)}
          className="workspace-source-chip-main"
        >
          <Icon
            name={kindIcon(item.object.kind)}
            className="h-3.5 w-3.5 shrink-0 text-ink-muted"
          />
          <span>{name}</span>
        </button>
      </Tooltip>
      <button
        type="button"
        aria-label={`Remove ${name} from context`}
        disabled={disabled}
        onClick={onRemove}
        className="workspace-source-chip-remove"
      >
        <Icon name="close" className="h-3 w-3" />
      </button>
      <Menu
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={trigger}
        label={`Role for ${name}`}
        side="top"
        align="start"
      >
        <MenuLabel>Use as</MenuLabel>
        {roles.map((value) => (
          <MenuItem
            key={value}
            checked={item.role === value}
            onSelect={() => {
              if (item.role !== value) onRole(value);
            }}
          >
            {sourceRoleLabels[value]}
          </MenuItem>
        ))}
      </Menu>
    </span>
  );
}

export default function WorkspaceContextTray({
  workspaceId,
  sessionId,
  disabled,
  onError,
}: {
  workspaceId: string;
  sessionId: string;
  disabled: boolean;
  onError: (message: string) => void;
}) {
  const [selection, setSelection] = useState<ContextSelection>({
    revision: 0,
    items: [],
  });
  const [busy, setBusy] = useState(false);
  const [loadedSession, setLoadedSession] = useState<string | null>(null);
  const [names, setNames] = useState<Record<string, string>>({});
  const [pending, setPending] = useState<OpenResearchObject[]>([]);
  const [moreOpen, setMoreOpen] = useState(false);
  const more = useRef<HTMLButtonElement>(null);
  const [row, rowWidth] = useContainerWidth<HTMLDivElement>();
  const scope = useRef(0);
  const saving = useRef(false);
  useEffect(() => {
    const epoch = ++scope.current;
    saving.current = false;
    setLoadedSession(null);
    setSelection({ revision: 0, items: [] });
    setNames({});
    setPending([]);
    setMoreOpen(false);
    setBusy(false);
    void deskClient
      .context(sessionId)
      .then((value) => {
        if (scope.current === epoch) {
          setSelection(value);
          setLoadedSession(sessionId);
        }
      })
      .catch((e) => {
        if (scope.current === epoch) onError(workbenchErrorMessage(e));
      });
    return () => {
      ++scope.current;
    };
  }, [sessionId, workspaceId, onError]);
  useEffect(() => {
    let alive = true;
    void Promise.all(
      selection.items.map(
        async (item) =>
          [
            key(item.object),
            (await deskClient.read(workspaceId, item.object)).title,
          ] as const,
      ),
    )
      .then((pairs) => {
        if (alive) setNames(Object.fromEntries(pairs));
      })
      .catch((e) => {
        if (alive) onError(workbenchErrorMessage(e));
      });
    return () => {
      alive = false;
    };
  }, [selection, workspaceId, onError]);
  const save = useCallback(
    async (items: ContextItem[]) => {
      if (disabled || saving.current || loadedSession !== sessionId) return;
      const epoch = scope.current;
      saving.current = true;
      setBusy(true);
      try {
        const updated = await deskClient.saveContext(
          sessionId,
          selection,
          items,
        );
        if (scope.current === epoch) setSelection(updated);
      } catch (e) {
        if (scope.current !== epoch) return;
        onError(workbenchErrorMessage(e));
        try {
          const current = await deskClient.context(sessionId);
          if (scope.current === epoch) setSelection(current);
        } catch (reloadError) {
          if (scope.current === epoch) {
            setLoadedSession(null);
            onError(workbenchErrorMessage(reloadError));
          }
        }
      } finally {
        if (scope.current === epoch) {
          saving.current = false;
          setBusy(false);
        }
      }
    },
    [disabled, loadedSession, sessionId, selection, onError],
  );
  const ready = !disabled && !busy && loadedSession === sessionId;
  const add = useCallback(
    (objects: OpenResearchObject[]) => {
      const fresh = objects.filter(
        (object, index) =>
          objects.findIndex((other) => key(other) === key(object)) === index &&
          !selection.items.some((item) => key(item.object) === key(object)),
      );
      if (!fresh.length) return;
      void save([
        ...selection.items,
        ...asContextItems(fresh, selection.items.length),
      ]);
    },
    [save, selection],
  );
  useEffect(() => {
    return appEvents.on("context-add", (detail) => {
      if (detail.workspaceId !== workspaceId) return;
      if (disabled || loadedSession !== sessionId || saving.current) {
        if (detail.whenReady)
          setPending((current) => [...current, ...detail.objects]);
        else
          onError(
            "Wait for the conversation and its sources to finish updating.",
          );
        return;
      }
      add(detail.objects);
    });
  }, [workspaceId, sessionId, loadedSession, disabled, onError, add]);
  // Sources held back during an import are added once changes are possible.
  useEffect(() => {
    if (!ready || !pending.length || saving.current) return;
    setPending([]);
    add(pending);
  }, [ready, pending, add]);

  // Show every source when all fit; otherwise leave room for "+N more".
  const fits = visibleFor(rowWidth);
  const visible =
    selection.items.length <= fits ? selection.items.length : fits;
  const hidden = selection.items.length - visible;
  useEffect(() => {
    if (hidden <= 0) setMoreOpen(false);
  }, [hidden]);

  // An empty or still-loading source list takes no space.
  const empty = loadedSession !== sessionId || !selection.items.length;
  const locked = disabled || busy;
  const chip = (item: ContextItem, index: number) => (
    <SourceChip
      key={key(item.object)}
      name={names[key(item.object)] ?? item.object.kind}
      item={item}
      disabled={locked}
      onRole={(role) =>
        void save(
          selection.items.map((value, n) =>
            n === index ? { ...value, role } : value,
          ),
        )
      }
      onRemove={() => void save(selection.items.filter((_, n) => n !== index))}
    />
  );
  return (
    <div
      ref={row}
      hidden={empty}
      role="group"
      aria-label="Conversation sources"
      className="workspace-source-row"
    >
      {selection.items.slice(0, visible).map(chip)}
      {hidden > 0 && (
        <>
          <button
            ref={more}
            type="button"
            aria-haspopup="dialog"
            aria-expanded={moreOpen}
            onClick={() => setMoreOpen((value) => !value)}
            className="workspace-source-more"
          >
            +{hidden} more
          </button>
          <Popover
            open={moreOpen}
            onClose={() => setMoreOpen(false)}
            anchorRef={more}
            label="More sources"
            side="top"
            align="end"
            width={280}
          >
            <div className="workspace-source-overflow">
              {selection.items
                .slice(visible)
                .map((item, index) => chip(item, index + visible))}
            </div>
          </Popover>
        </>
      )}
    </div>
  );
}
