import { useEffect, useId, useRef, useState } from "react";

/** Shared button-tab semantics; manual activation avoids loading data on arrows. */
export default function useTabList<T extends string>(
  ids: readonly T[],
  selected: T,
  onSelect: (id: T) => void,
  activation: "automatic" | "manual" = "automatic",
) {
  const instance = useId();
  const refs = useRef(new Map<T, HTMLButtonElement>());
  const [focused, setFocused] = useState(selected);
  useEffect(() => setFocused(selected), [selected]);
  const current = ids.includes(focused)
    ? focused
    : ids.includes(selected)
      ? selected
      : ids[0];
  const tabId = (id: T) => `${instance}-tab-${encodeURIComponent(id)}`;
  const panelId = (id: T) => `${instance}-panel-${encodeURIComponent(id)}`;
  const focus = (id: T) => {
    setFocused(id);
    refs.current.get(id)?.focus();
  };
  const tabProps = (id: T) => ({
    id: tabId(id),
    type: "button" as const,
    role: "tab" as const,
    "aria-selected": id === selected,
    "aria-controls": panelId(id),
    tabIndex: id === current ? 0 : -1,
    ref: (node: HTMLButtonElement | null) => {
      if (node) refs.current.set(id, node);
      else refs.current.delete(id);
    },
    onFocus: () => setFocused(id),
    onClick: () => onSelect(id),
    onKeyDown: (event: React.KeyboardEvent<HTMLButtonElement>) => {
      const index = ids.indexOf(id);
      let next: T | undefined;
      if (event.key === "ArrowRight") next = ids[(index + 1) % ids.length];
      if (event.key === "ArrowLeft")
        next = ids[(index - 1 + ids.length) % ids.length];
      if (event.key === "Home") next = ids[0];
      if (event.key === "End") next = ids[ids.length - 1];
      if (!next) return;
      event.preventDefault();
      focus(next);
      if (activation === "automatic") onSelect(next);
    },
  });
  const panelProps = (id: T) => ({
    id: panelId(id),
    role: "tabpanel" as const,
    "aria-labelledby": tabId(id),
    tabIndex: 0,
  });
  return { tabProps, panelProps, tabId, panelId, focus };
}
