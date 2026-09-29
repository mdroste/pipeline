import { useEffect, useState } from "react";
import { projectClient, type ProjectCapabilities } from "../lib/projectClient";
import ChatgptConnection from "./ChatgptConnection";
import ConversationTitleSettings from "./settings/ConversationTitleSettings";

export default function WorkspaceConnectionSettings({
  embedded = false,
  connectionOnly = false,
}: {
  embedded?: boolean;
  connectionOnly?: boolean;
}) {
  const [capabilities, setCapabilities] = useState<ProjectCapabilities | null>(
    null,
  );
  useEffect(() => {
    if (connectionOnly) return;
    let live = true;
    void projectClient
      .capabilities()
      .then((value) => {
        if (live) setCapabilities(value);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [connectionOnly]);
  return (
    <div className="max-w-3xl space-y-8">
      {!embedded && <h1 className="text-xl font-semibold">ChatGPT account</h1>}
      <ChatgptConnection />
      {!connectionOnly && <ConversationTitleSettings />}

      {!connectionOnly && (
        <section className="space-y-3 rounded-xl border p-5 text-sm">
          <h2 className="font-semibold">Research capabilities</h2>
          <p className="text-gray-500">
            {capabilities?.qualification ??
              "Project reading, notes and task history work without ChatGPT sign-in."}
          </p>
          {capabilities && (
            <>
              <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-xs">
                <dt>Platform</dt>
                <dd>
                  {capabilities.platform} · file acceptance{" "}
                  {capabilities.fileAcceptance
                    ? "available"
                    : "not yet supported on this platform"}
                </dd>
                <dt>Git</dt>
                <dd>
                  {capabilities.git ??
                    "Missing. Install Git or use local file copies."}
                </dd>
                <dt>LaTeX</dt>
                <dd>
                  {capabilities.latex ??
                    "Missing. Install a TeX distribution and configure an execution profile."}
                </dd>
                <dt>PDF pages</dt>
                <dd>
                  {capabilities.pdfPages ??
                    "No system pdftoppm found. Packaged resources may still supply it; try a page render, or install Poppler."}
                </dd>
                <dt>Stata</dt>
                <dd>{capabilities.stataPolicy}</dd>
              </dl>
              <p className="text-xs text-gray-500">
                Test each command profile before using it. Platform test
                details: {capabilities.record}
              </p>
            </>
          )}
        </section>
      )}
    </div>
  );
}
