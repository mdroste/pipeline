import { useCallback, useEffect, useState } from "react";

export type NavSection = "projects" | "reviews";

const STORAGE_KEY = "pipeline.ui.navRailSections";

type Choices = Partial<Record<NavSection, boolean>>;

function readChoices(): Choices {
  try {
    const raw: unknown = JSON.parse(
      localStorage.getItem(STORAGE_KEY) ?? "null",
    );
    if (raw && typeof raw === "object") {
      const { projects, reviews } = raw as Choices;
      return Object.fromEntries(
        Object.entries({ projects, reviews }).filter(
          ([, open]) => typeof open === "boolean",
        ),
      );
    }
  } catch {
    /* Corrupt or inaccessible optional preferences use defaults. */
  }
  return {};
}

/**
 * Which sidebar sections are open. The section holding the current page opens
 * by itself; the chevrons record a choice that outlasts the visit. Arriving in
 * a section clears a choice to keep it shut, so a click never lands on a
 * closed section.
 */
export default function useNavSections(
  active: NavSection | null,
  transient = false,
) {
  const [choices, setChoices] = useState(readChoices);
  useEffect(() => {
    if (!active || transient) return;
    setChoices((current) => {
      if (current[active] !== false) return current;
      const { [active]: _cleared, ...rest } = current;
      return rest;
    });
  }, [active, transient]);
  useEffect(() => {
    if (transient) return;
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(choices));
    } catch {
      /* Optional view preference. */
    }
  }, [choices, transient]);
  const isOpen = useCallback(
    (section: NavSection) =>
      transient && section === active
        ? true
        : (choices[section] ?? section === active),
    [active, choices, transient],
  );
  const toggle = useCallback(
    (section: NavSection) =>
      setChoices((current) => ({
        ...current,
        [section]: !(current[section] ?? section === active),
      })),
    [active],
  );
  return { isOpen, toggle };
}
