// Open files arranged in one pane or two. Each pane is a group with its own
// tabs and its own active file. A second pane that loses its last tab closes,
// and the files it held are never lost by closing a pane.

import type { FileLocation } from "../../lib/fileLinks";

export interface FileGroup {
  tabs: string[];
  active: FileLocation | null;
}
/** Side by side, or one pane above the other. */
export type SplitDirection = "horizontal" | "vertical";
export interface FileLayout {
  /** One group, or two when the editor is split. */
  groups: FileGroup[];
  /** The group that receives files opened from the list or a link. */
  focus: number;
  direction: SplitDirection;
}

const MAX_TABS = 30;
const EMPTY: FileGroup = { tabs: [], active: null };

const replaced = (groups: FileGroup[], index: number, group: FileGroup) =>
  groups.map((old, i) => (i === index ? group : old));

/** Closes empty groups but keeps one, and keeps focus on the same group. */
function tidy(layout: FileLayout): FileLayout {
  if (layout.groups.length < 2 || layout.groups.every((g) => g.tabs.length))
    return layout;
  const groups = layout.groups.filter((g) => g.tabs.length);
  if (!groups.length) return { ...layout, groups: [EMPTY], focus: 0 };
  return {
    ...layout,
    groups,
    focus: Math.max(0, groups.indexOf(layout.groups[layout.focus])),
  };
}

/** `group` without the `gone` tabs. A tab beside the active one takes over. */
function without(group: FileGroup, gone: ReadonlySet<string>): FileGroup {
  if (!group.tabs.some((path) => gone.has(path))) return group;
  const tabs = group.tabs.filter((path) => !gone.has(path));
  if (!group.active || !gone.has(group.active.path))
    return { tabs, active: group.active };
  const at = Math.max(0, group.tabs.indexOf(group.active.path));
  const next = tabs[Math.min(at, tabs.length - 1)];
  return { tabs, active: next ? { path: next } : null };
}

/** Reads a saved layout, including the single-pane form older versions saved. */
export function restoreLayout(saved: unknown, allowSplit: boolean): FileLayout {
  const value = saved as Record<string, unknown> | null;
  const group = (raw: unknown): FileGroup | null => {
    const g = raw as { tabs?: unknown; active?: unknown } | null;
    const active = g?.active as { path?: unknown } | null | undefined;
    if (
      !g ||
      !Array.isArray(g.tabs) ||
      !g.tabs.every((path) => typeof path === "string") ||
      !(active === null || typeof active?.path === "string")
    )
      return null;
    return {
      tabs: [...new Set<string>(g.tabs)].slice(0, MAX_TABS),
      active: active as FileLocation | null,
    };
  };
  let groups: FileGroup[] = [];
  let focus = 0;
  let direction: SplitDirection = "horizontal";
  if (Array.isArray(value?.groups)) {
    groups = value.groups
      .slice(0, 2)
      .map(group)
      .filter((g) => g !== null);
    focus = value.focus === 1 ? 1 : 0;
    direction = value.direction === "vertical" ? "vertical" : "horizontal";
  } else {
    const single = group(value);
    if (single) groups = [single];
    // Older versions kept one path in a second pane beside the tabs.
    if (single && typeof value?.split === "string")
      groups.push({ tabs: [value.split], active: { path: value.split } });
  }
  if (!groups.length) groups = [EMPTY];
  let layout = tidy({
    groups,
    focus: Math.min(focus, groups.length - 1),
    direction,
  });
  if (!allowSplit) layout = closePane(layout, 1);
  return layout;
}

/** Shows `location` in `group`, adding its tab there if it has none. */
export function showIn(
  layout: FileLayout,
  location: FileLocation,
  group = layout.focus,
): FileLayout {
  const target = layout.groups[group] ?? layout.groups[0];
  const index = layout.groups.indexOf(target);
  const tabs = target.tabs.includes(location.path)
    ? target.tabs
    : [...target.tabs, location.path];
  return {
    ...layout,
    focus: index,
    groups: replaced(layout.groups, index, { tabs, active: location }),
  };
}

/** Closes tabs in one group, or in every group when `group` is left out. */
export function closeTabs(
  layout: FileLayout,
  paths: readonly string[],
  group?: number,
): FileLayout {
  const gone = new Set(paths);
  return tidy({
    ...layout,
    groups: layout.groups.map((g, i) =>
      group === undefined || i === group ? without(g, gone) : g,
    ),
  });
}

/**
 * Moves a tab to `index` within its group, or into the other group at
 * `index` (its end when left out), where it becomes the active file.
 */
export function moveTab(
  layout: FileLayout,
  path: string,
  from: number,
  to: number,
  index?: number,
): FileLayout {
  const source = layout.groups[from];
  const target = layout.groups[to];
  if (!source?.tabs.includes(path) || !target) return layout;
  const rest = target.tabs.filter((p) => p !== path);
  const at = Math.max(0, Math.min(index ?? rest.length, rest.length));
  rest.splice(at, 0, path);
  if (from === to)
    return rest.every((p, i) => p === source.tabs[i])
      ? layout
      : {
          ...layout,
          groups: replaced(layout.groups, from, { ...source, tabs: rest }),
        };
  const location = source.active?.path === path ? source.active : { path };
  let groups = replaced(layout.groups, from, without(source, new Set([path])));
  groups = replaced(groups, to, { tabs: rest, active: location });
  return tidy({ ...layout, groups, focus: to });
}

/**
 * Opens a tab in a new pane beside or below the only one. A dragged tab moves
 * there, unless it is the last one in its pane; with `copy` it stays as well.
 */
export function splitOff(
  layout: FileLayout,
  path: string,
  direction: SplitDirection,
  copy = false,
): FileLayout {
  const [source] = layout.groups;
  if (layout.groups.length > 1 || !source.tabs.includes(path)) return layout;
  const location = source.active?.path === path ? source.active : { path };
  return {
    direction,
    focus: 1,
    groups: [
      copy || source.tabs.length < 2
        ? source
        : without(source, new Set([path])),
      { tabs: [path], active: location },
    ],
  };
}

/** Closes a pane. Its tabs join the other pane, which keeps its file open. */
export function closePane(layout: FileLayout, group: number): FileLayout {
  if (layout.groups.length < 2 || !layout.groups[group]) return layout;
  const other = layout.groups[1 - group];
  const tabs = [
    ...other.tabs,
    ...layout.groups[group].tabs.filter((path) => !other.tabs.includes(path)),
  ].slice(0, MAX_TABS);
  return { ...layout, focus: 0, groups: [{ tabs, active: other.active }] };
}

/** Points tabs and active files at new paths after a rename or move. */
export function remapLayout(
  layout: FileLayout,
  carry: (path: string) => string,
): FileLayout {
  return {
    ...layout,
    groups: layout.groups.map((g) => ({
      tabs: [...new Set(g.tabs.map(carry))],
      active: g.active ? { ...g.active, path: carry(g.active.path) } : null,
    })),
  };
}
