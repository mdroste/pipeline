// The reading text of a Word document. Layout stays in Word.
import { useEffect, useState } from "react";
import type {
  DocumentText,
  FileAdapter,
} from "../../../lib/fileWorkspaceClient";
import { workbenchErrorMessage } from "../../../lib/workbenchError";

export default function DocumentTextViewer({
  adapter,
  path,
  version,
}: {
  adapter: FileAdapter;
  path: string;
  version: string;
}) {
  const [document, setDocument] = useState<DocumentText | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    setDocument(null);
    setError("");
    if (!adapter.documentText) {
      setError("This document cannot be read here. Open it externally.");
      return;
    }
    adapter.documentText(path).then(
      (value) => live && setDocument(value),
      (e) => live && setError(workbenchErrorMessage(e)),
    );
    return () => {
      live = false;
    };
  }, [adapter, path, version]);
  if (error)
    return (
      <p role="alert" className="file-error">
        {error}
      </p>
    );
  if (!document) return <p className="file-status">Reading document…</p>;
  return (
    <div className="file-document">
      <p role="note" className="file-status">
        Text only. Open externally to see the page layout.
        {document.truncated ? " This view stops at 2 MB of text." : ""}
      </p>
      <article>{document.text}</article>
    </div>
  );
}
