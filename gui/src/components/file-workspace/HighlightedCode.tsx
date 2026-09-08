import { useMemo, useState } from "react";
import { highlightedSource, languageId } from "../../lib/fileLanguages";
import "./files.css";

export default function HighlightedCode({
  text,
  language = "plaintext",
  copy = false,
}: {
  text: string;
  language?: string;
  copy?: boolean;
}) {
  const html = useMemo(
    () => highlightedSource(text, language),
    [text, language],
  );
  const [status, setStatus] = useState("");
  return (
    <div className="file-code-block">
      {copy && (
        <div className="file-code-toolbar">
          <span>{languageId(language)}</span>
          <button
            type="button"
            onClick={() =>
              void navigator.clipboard.writeText(text).then(
                () => setStatus("Copied"),
                () => setStatus("Copy unavailable"),
              )
            }
          >
            {status || "Copy code"}
          </button>
        </div>
      )}
      <pre>
        {html === null ? (
          <code>{text}</code>
        ) : (
          <code dangerouslySetInnerHTML={{ __html: html }} />
        )}
      </pre>
    </div>
  );
}
