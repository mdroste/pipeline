import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  deskClient,
  type Acquisition,
  type DeskRecord,
} from "../../lib/deskClient";
import { workbenchClient } from "../../lib/workbenchClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { button, input, card, muted, type DeskProps } from "./shared";
export default function Discovery({ workspaceId, onOpen, onError }: DeskProps) {
  const [enabled, setEnabled] = useState(false);
  const [receipts, setReceipts] = useState<DeskRecord<Acquisition>[]>([]);
  const [query, setQuery] = useState("");
  const [url, setUrl] = useState("");
  const [title, setTitle] = useState("");
  const [citation, setCitation] = useState("");
  const [busy, setBusy] = useState(false);
  const refresh = useCallback(async () => {
    const [network, items] = await Promise.all([
      deskClient.network(workspaceId),
      deskClient.records<Acquisition>(workspaceId, "acquisition"),
    ]);
    setEnabled(network);
    setReceipts(
      items.filter(
        (item) => !items.some((next) => next.supersedes === item.id),
      ),
    );
  }, [workspaceId]);
  useEffect(() => {
    void refresh().catch((e) => onError(workbenchErrorMessage(e)));
  }, [refresh, onError]);
  const run = async (fn: () => Promise<unknown>) => {
    if (busy) return;
    setBusy(true);
    try {
      await fn();
      await refresh();
    } catch (e) {
      onError(workbenchErrorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const local = () =>
    run(async () => {
      const path = await open({
        multiple: false,
        filters: [{ name: "Source PDF", extensions: ["pdf"] }],
      });
      if (typeof path !== "string") return;
      await workbenchClient.importSource({
        workspaceId,
        title: title || path.split("/").pop() || "Source PDF",
        citationKey: citation || null,
        identifiers: {},
        versionLabel: null,
        path,
        locator: null,
        accessState: "partial",
        acquiredVia: "local_pdf",
        operationId: `source-${crypto.randomUUID()}`,
      });
    });
  const openSource = async (id: string) => {
    const sources = await workbenchClient.listSources(workspaceId);
    const source = sources.find((s) => s.versionId === id);
    if (!source) throw new Error("Source version is unavailable");
    onOpen({
      kind: "source",
      id,
      revision: source.contentHash ?? source.createdAt,
    });
  };
  return (
    <div className="space-y-5">
      <section className={card}>
        <h2 className="font-semibold">Literature acquisition</h2>
        <label className="flex gap-2 text-sm">
          <input
            type="checkbox"
            checked={enabled}
            disabled={busy}
            onChange={(e) =>
              void run(() => deskClient.network(workspaceId, e.target.checked))
            }
          />
          Allow host literature and data acquisition for this project
        </label>
        <p className={muted}>
          Queries go to the selected provider. This setting is separate from
          conversation web search.
        </p>
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            void run(() => deskClient.lookup(workspaceId, query));
          }}
        >
          <input
            aria-label="Literature query"
            className={input}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="DOI, title, author, or research question"
          />
          <button
            className={button}
            disabled={!enabled || busy || !query.trim()}
          >
            Find with Crossref
          </button>
        </form>
        <input
          aria-label="Citation key"
          className={input}
          value={citation}
          onChange={(e) => setCitation(e.target.value)}
          placeholder="Optional citation key for selected import"
        />
      </section>
      <section className={card}>
        <h2 className="font-semibold">Attach a source</h2>
        <input
          aria-label="Source title"
          className={input}
          placeholder="Source title"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
        />
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            void run(() => deskClient.pdf(workspaceId, title, url));
          }}
        >
          <input
            aria-label="Direct PDF URL"
            className={input}
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://…/paper.pdf"
          />
          <button
            className={button}
            disabled={!enabled || busy || !title.trim() || !url.trim()}
          >
            Download PDF
          </button>
        </form>
        <button className={button} disabled={busy} onClick={() => void local()}>
          Import local PDF
        </button>
        <p className={muted}>
          Use Literature to import Zotero collections, add citation notes, and
          compare paper versions. Check the full text before treating a source
          as evidence.
        </p>
      </section>
      <h2 className="font-semibold">Reading inbox</h2>
      {receipts.slice(0, 60).map((r) => (
        <article className={card} key={r.id}>
          <h3 className="font-semibold">{r.title}</h3>
          <p className={muted}>
            {r.body.provider} · {r.body.state} · {r.body.access} ·{" "}
            {new Date(r.createdAt).toLocaleString()}
          </p>
          {r.body.error && (
            <p role="alert" className="text-sm text-red-700">
              {r.body.error}
            </p>
          )}
          {r.body.sourceVersionId && (
            <button
              className={button}
              onClick={() =>
                void openSource(r.body.sourceVersionId!).catch((e) =>
                  onError(workbenchErrorMessage(e)),
                )
              }
            >
              Read saved source
            </button>
          )}
          {r.body.sourceVersionId && (
            <div className="flex gap-2">
              <button
                className={button}
                disabled={busy}
                onClick={() =>
                  void run(() =>
                    deskClient.inboxState(workspaceId, r.id, "read"),
                  )
                }
              >
                Mark read
              </button>
              <button
                className={button}
                disabled={busy}
                onClick={() =>
                  void run(() =>
                    deskClient.inboxState(
                      workspaceId,
                      r.id,
                      r.body.state === "excluded" ? "read" : "excluded",
                    ),
                  )
                }
              >
                {r.body.state === "excluded"
                  ? "Include again"
                  : "Exclude source"}
              </button>
            </div>
          )}
          {r.body.candidates?.map((c, i) => (
            <details key={i} className="rounded border p-3">
              <summary>
                {c.title} · {c.year ?? "Year unavailable"}
              </summary>
              <p className="my-2 text-xs">
                {c.authors.join(", ")} · {c.doi}
              </p>
              <p className="my-2 text-sm">
                {c.abstractText ?? "No abstract supplied. Metadata only."}
              </p>
              <button
                className={button}
                disabled={busy}
                onClick={() =>
                  void run(() =>
                    deskClient.candidate(
                      workspaceId,
                      r.id,
                      i,
                      citation || null,
                    ),
                  )
                }
              >
                Import this candidate
              </button>
            </details>
          ))}
        </article>
      ))}
    </div>
  );
}
