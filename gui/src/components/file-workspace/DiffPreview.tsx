import { useState } from "react";
import HighlightedCode from "./HighlightedCode";

const SECTION_CHARACTERS = 12_000;

/** Bound DOM/highlighter work even when the diff falls back to a whole file.
 * Only the preview is paged; selection always refers to the complete hunk. */
export default function DiffPreview({
  text,
  language,
  label,
}: {
  text: string;
  language: string;
  label: string;
}) {
  const [page, setPage] = useState(0);
  const start = Math.min(
    page * SECTION_CHARACTERS,
    Math.max(0, text.length - 1),
  );
  const end = Math.min(start + SECTION_CHARACTERS, text.length);
  // Include the other half of a surrogate at a boundary in both neighboring
  // previews, so supplementary characters never display as replacement glyphs.
  const from =
    start > 0 && /[\uDC00-\uDFFF]/.test(text[start]) ? start - 1 : start;
  const to =
    end < text.length && /[\uDC00-\uDFFF]/.test(text[end]) ? end + 1 : end;
  return (
    <>
      {text.length > SECTION_CHARACTERS && (
        <div className="file-toolbar">
          <button
            aria-label={`${label}: previous preview section`}
            disabled={page === 0}
            onClick={() => setPage(page - 1)}
          >
            Previous section
          </button>
          <span>
            Characters {from + 1}–{to} of {text.length}. Selection includes the
            entire change.
          </span>
          <button
            aria-label={`${label}: next preview section`}
            disabled={end === text.length}
            onClick={() => setPage(page + 1)}
          >
            Next section
          </button>
        </div>
      )}
      <HighlightedCode text={text.slice(from, to)} language={language} />
    </>
  );
}
