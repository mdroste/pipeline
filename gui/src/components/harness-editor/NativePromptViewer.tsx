import { useEffect, useState } from "react";
import { workbenchClient } from "../../lib/workbenchClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import type { NativePromptCatalog } from "../../lib/workbenchTypes";

export default function NativePromptViewer({ onUseTemplate, busy }: { onUseTemplate?: (text: string) => void; busy: boolean }) {
  const [catalog, setCatalog] = useState<NativePromptCatalog | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [sourceIndex, setSourceIndex] = useState(0);
  const [modelId, setModelId] = useState("");
  useEffect(() => {
    let live = true;
    workbenchClient.nativePromptCatalog().then(result => {
      if (live) { setCatalog(result); setError(null); }
    }).catch(cause => { if (live) setError(workbenchErrorMessage(cause)); });
    return () => { live = false; };
  }, [attempt]);
  const source = catalog?.sources[sourceIndex] ?? catalog?.sources[0];
  const model = source?.models.find(item => item.model === modelId) ?? source?.models[0];
  return <div className="space-y-3 text-xs">
    <p className="text-gray-500 dark:text-gray-400">Read-only model templates from local Codex metadata. This is a cached reference, not the fully assembled prompt for a turn. Runtime instructions, tools, permissions, and project instructions are added separately. Variables are shown verbatim.</p>
    {error && <p role="alert">{error}</p>}
    {!catalog && !error && <p role="status">Reading local prompt cache…</p>}
    {catalog && <>
      <p className="text-gray-500">Installed Codex: {catalog.installedVersion ?? "unknown"}</p>
      {catalog.diagnostics.map(note => <p key={note} className="break-words text-amber-700 dark:text-amber-300">{note}</p>)}
      {source && model ? <>
        <label className="block">Prompt source
          <select aria-label="Prompt source" value={catalog.sources.indexOf(source)} onChange={event => { setSourceIndex(Number(event.target.value)); setModelId(""); }} className="mt-1 block w-full rounded border border-gray-300 bg-white p-2 dark:border-gray-600 dark:bg-gray-800">
            {catalog.sources.map((item, index) => <option key={item.path} value={index}>{item.origin === "pipeline" ? "Pipeline runtime cache" : "Codex installation cache · reference only"}</option>)}
          </select>
        </label>
        {source.origin === "codex" && <p className="text-amber-700 dark:text-amber-300">This cache belongs to the regular Codex installation. It does not establish which default Pipeline’s runtime is using.</p>}
        <p className="break-all text-gray-500">{source.path}<br />Cache client: {source.clientVersion ?? "unknown"} · Fetched: {source.fetchedAt ?? "unknown"}</p>
        {catalog.installedVersion && source.clientVersion && catalog.installedVersion !== source.clientVersion && <p className="text-amber-700 dark:text-amber-300">The cache version differs from the installed CLI version.</p>}
        <label className="block">Model template
          <select aria-label="Model template" value={model.model} onChange={event => setModelId(event.target.value)} className="mt-1 block w-full rounded border border-gray-300 bg-white p-2 dark:border-gray-600 dark:bg-gray-800">
            {source.models.map(item => <option key={item.model} value={item.model}>{item.model}</option>)}
          </select>
        </label>
        <p className="break-all text-[10px] text-gray-500">Source field: {model.templateField}</p>
        <pre aria-label="Codex native base prompt" className="max-h-80 overflow-auto whitespace-pre-wrap rounded border border-gray-200 bg-gray-50 p-3 leading-relaxed dark:border-gray-700 dark:bg-gray-800">{model.template}</pre>
        {onUseTemplate && <button type="button" disabled={busy} onClick={() => onUseTemplate(model.template)} className="rounded border border-gray-300 px-3 py-1.5 disabled:opacity-50 dark:border-gray-600">Use this template as replacement</button>}
        {model.sections.length > 0 && <details>
          <summary className="cursor-pointer font-medium">Other cached instruction sections ({model.sections.length})</summary>
          <p className="my-2 text-gray-500">The runtime selects these according to its configuration. Replacing the base prompt does not replace every section listed here.</p>
          {model.sections.map(section => <details key={section.id} className="my-2 rounded border border-gray-200 dark:border-gray-700">
            <summary className="cursor-pointer break-all p-2">{section.label}</summary>
            <pre className="max-h-60 overflow-auto whitespace-pre-wrap p-3 leading-relaxed">{section.text}</pre>
          </details>)}
        </details>}
      </> : <p>No supported prompt cache is available. Connect Codex in Workspace settings and refresh after its model catalog loads. You can still supply your own replacement prompt.</p>}
    </>}
    <button type="button" onClick={() => setAttempt(value => value + 1)} className="rounded border border-gray-300 px-2 py-1 dark:border-gray-600">Refresh prompt cache</button>
  </div>;
}
