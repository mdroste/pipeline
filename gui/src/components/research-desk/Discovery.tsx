import { Fold } from "../../ui";
import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  deskClient,
  type Acquisition,
  type DeskRecord,
} from "../../lib/deskClient";
import { researchClient } from "../../lib/workbench/researchClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import useReopenCount from "../../hooks/useReopenCount";
import { folderName } from "../project-surface/shared";
import { plainLabel } from "../research-panel/format";
import { button, input, card, muted, type DeskProps } from "./shared";

const INBOX_PAGE = 60;

export default function Discovery({
  workspaceId,
  onOpen,
  onError,
  onChanged,
  onActionStart,
  active,
}: DeskProps) {
  const [enabled, setEnabled] = useState(false);
  const [receipts, setReceipts] = useState<DeskRecord<Acquisition>[]>([]);
  const [shown, setShown] = useState(INBOX_PAGE);
  const [query, setQuery] = useState("");
  const [url, setUrl] = useState("");
  const [title, setTitle] = useState("");
  const [citation, setCitation] = useState("");
  const [contact, setContact] = useState("");
  const [savedContact, setSavedContact] = useState<string | null>(null);
  const loadedContact = useRef("");
  const [busy, setBusy] = useState(false);
  const refresh = useCallback(async () => {
    const [network, items, email] = await Promise.all([
      deskClient.network(workspaceId),
      deskClient.records<Acquisition>(workspaceId, "acquisition"),
      deskClient.crossrefContact(),
    ]);
    setEnabled(network);
    setSavedContact(email);
    // A reload must not replace an address the researcher is still typing.
    const previous = loadedContact.current;
    const next = email ?? "";
    loadedContact.current = next;
    setContact((current) =>
      current === previous || current.trim() === next ? next : current,
    );
    setReceipts(
      items.filter(
        (item) => !items.some((next) => next.supersedes === item.id),
      ),
    );
  }, [workspaceId]);
  const reopened = useReopenCount(active);
  useEffect(() => {
    void refresh().catch((e) => onError(workbenchErrorMessage(e)));
  }, [refresh, onError, reopened]);
  const run = async (fn: () => Promise<unknown>) => {
    if (busy) return;
    setBusy(true);
    onActionStart?.();
    try {
      await fn();
      onChanged?.();
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
      await researchClient.importSource({
        workspaceId,
        title: title || folderName(path) || "Source PDF",
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
    const sources = await researchClient.listSources(workspaceId);
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
      {receipts.length > 0 && <h2 className="font-semibold">Reading inbox</h2>}
      {receipts.slice(0, shown).map((r) => (
        <article className={card} key={r.id}>
          <h3 className="font-semibold">{r.title}</h3>
          <p className={muted}>
            {r.body.provider} · {plainLabel(r.body.state)}
            {r.body.access ? ` · ${plainLabel(r.body.access)}` : ""} ·{" "}
            {new Date(r.createdAt).toLocaleString()}
          </p>
          {r.body.error && (
            <p role="alert" className="text-sm text-danger">
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
                Add this source
              </button>
            </details>
          ))}
        </article>
      ))}
      {receipts.length > shown && (
        <div className="flex items-center gap-2">
          <p className={muted}>
            Showing the {shown} newest of {receipts.length} items.
          </p>
          <button
            className={button}
            onClick={() => setShown((count) => count + INBOX_PAGE)}
          >
            Show older
          </button>
        </div>
      )}
      <Fold title="Find literature" open={!receipts.length}>
        <label className="flex gap-2 text-sm">
          <input
            type="checkbox"
            checked={enabled}
            disabled={busy}
            onChange={(e) =>
              void run(() => deskClient.network(workspaceId, e.target.checked))
            }
          />
          Allow Pipeline to search for literature and download data for this
          project
        </label>
        <p className={muted}>
          Queries go to the selected provider. This setting is separate from
          conversation web search.
        </p>
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            void run(() =>
              deskClient.crossrefContact(contact.trim() || null, true),
            );
          }}
        >
          <input
            aria-label="Crossref contact email"
            type="email"
            autoComplete="email"
            className={input}
            value={contact}
            onChange={(e) => setContact(e.target.value)}
            placeholder="Your email for Crossref (optional)"
          />
          <button
            className={button}
            disabled={busy || contact.trim() === (savedContact ?? "")}
          >
            {contact.trim() ? "Save contact" : "Remove contact"}
          </button>
        </form>
        <p className={muted}>
          Optional. Crossref asks API users for a contact address and may limit
          requests per address. When set, Pipeline sends your own email to
          Crossref with each literature lookup from this computer; it never uses
          a shared address. Leave it empty to search without a contact.
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
      </Fold>
      <Fold title="Attach a source">
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
      </Fold>
    </div>
  );
}
