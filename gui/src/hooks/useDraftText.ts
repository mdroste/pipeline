import { useEffect, useState } from "react";
import type { ChangeEvent, KeyboardEvent } from "react";
import { flushSync } from "react-dom";

type DraftElement = HTMLInputElement | HTMLTextAreaElement;

/**
 * Keep a field's raw text local while the user edits it.
 *
 * `value` is the canonical text derived from the owner's parsed state. The
 * draft reaches `commit` on blur (and on Enter for single-line fields), so
 * separators, spaces and partial numbers are never re-derived mid-typing.
 * Outside changes replace the draft only while the field is not being edited.
 * Escape restores the canonical text. Cmd/Ctrl+S commits the draft before a
 * window-level save shortcut reads the owner's state.
 */
export default function useDraftText(
  value: string,
  commit: (text: string) => void,
  { multiline = false }: { multiline?: boolean } = {},
) {
  const [draft, setDraft] = useState(value);
  const [editing, setEditing] = useState(false);
  useEffect(() => {
    if (!editing) setDraft(value);
  }, [value, editing]);
  const finish = (text: string) => {
    setEditing(false);
    if (text !== value) commit(text);
  };
  return {
    value: draft,
    onFocus: () => setEditing(true),
    onChange: (event: ChangeEvent<DraftElement>) => {
      setEditing(true);
      setDraft(event.target.value);
    },
    onBlur: () => finish(draft),
    onKeyDown: (event: KeyboardEvent<DraftElement>) => {
      if (event.nativeEvent.isComposing) return;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
        // Render the commit now, so a save listener on window, later in this
        // dispatch, reads the committed value rather than waiting on React's
        // scheduled flush. The event is not cancelled; that listener saves.
        flushSync(() => finish(draft));
      } else if (event.key === "Escape") {
        setDraft(value);
        setEditing(false);
      } else if (event.key === "Enter" && !multiline) {
        finish(draft);
      }
    },
  };
}
