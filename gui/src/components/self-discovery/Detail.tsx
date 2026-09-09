import { lazy, Suspense, useEffect, useRef, useState } from "react";
import {
  discoveryClient,
  discoveryNames,
  discoveryTerminal,
  type Candidate,
  type DiscoveryPaper,
  type DiscoveryRun,
} from "../../lib/discoveryClient";

import ResearchReaderBoundary from "../ResearchReaderBoundary";
const ReportViewer = lazy(() => import("../ReportViewer"));

export default function Detail({
  run,
  onUpdated,
  onClose,
  onConversation,
}: {
  run: DiscoveryRun;
  onUpdated: (r: DiscoveryRun) => void;
  onClose: () => void;
  onConversation?: (id: string) => void | Promise<void>;
}) {
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [selected, setSelected] = useState<number[]>([]);
  const [showCandidates, setShowCandidates] = useState(false);
  const [paper, setPaper] = useState<DiscoveryPaper | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const operation = useRef(crypto.randomUUID());
  const paperRequest = useRef(0);
  const choosing = run.state === "awaitingSelection";
  useEffect(() => {
    setSelected(run.selection?.recommended ?? []);
    operation.current = crypto.randomUUID();
  }, [run.id, run.selectionHash]);
  useEffect(() => {
    let live = true;
    if ((showCandidates || choosing) && run.candidateCount)
      void Promise.all(
        Array.from({ length: Math.ceil(run.candidateCount / 25) }, (_, i) =>
          discoveryClient.candidates(run.id, i * 25),
        ),
      )
        .then((pages) => {
          if (live) setCandidates(pages.flat());
        })
        .catch((e) => {
          if (live) setError(String(e));
        });
    return () => {
      live = false;
    };
  }, [run.id, run.candidateCount, run.selectionHash, choosing, showCandidates]);
  useEffect(
    () => () => {
      paperRequest.current++;
    },
    [],
  );
  async function act(work: () => Promise<DiscoveryRun | unknown>) {
    setBusy(true);
    setError("");
    try {
      const result = await work();
      if (result && typeof result === "object" && "revision" in result)
        onUpdated(result as DiscoveryRun);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  const choices = choosing
    ? candidates.filter((c) => run.selection?.shortlist.includes(c.id))
    : candidates;
  const current = paper?.versions[paper.versions.length - 1];
  return (
    <section
      className="task-detail discovery-detail"
      aria-label="Self-discovery progress"
    >
      <div className="task-section-heading">
        <h2>{discoveryNames[run.definition.mode]}</h2>
        <button aria-label="Close self-discovery details" onClick={onClose}>
          ×
        </button>
      </div>
      <p>{run.definition.prompt}</p>
      <p role="status">
        <strong>{run.phaseLabel}</strong> · {run.state}
      </p>
      <p>{run.reason}</p>
      <p className="task-muted">
        {run.candidateCount} candidates · {run.papers.length} projects developed
        · {run.actionsReserved}/{run.definition.maxActions} actions ·{" "}
        {(run.activeSeconds / 3600).toFixed(1)} active hours
      </p>
      <div className="task-actions">
        {run.state === "paused" ? (
          <button
            disabled={busy}
            onClick={() =>
              void act(() => discoveryClient.control(run, "resume"))
            }
          >
            Resume
          </button>
        ) : (
          ["running", "awaitingSelection"].includes(run.state) && (
            <button
              disabled={busy}
              onClick={() =>
                void act(() => discoveryClient.control(run, "pause"))
              }
            >
              Pause
            </button>
          )
        )}
        {!discoveryTerminal(run.state) && run.state !== "stopping" && (
          <button
            disabled={busy}
            onClick={() => void act(() => discoveryClient.control(run, "stop"))}
          >
            Stop
          </button>
        )}
        <button
          disabled={busy}
          onClick={() => void act(() => discoveryClient.export(run.id, "zip"))}
        >
          Export papers and research
        </button>
        <button
          disabled={busy}
          onClick={() =>
            void act(() => discoveryClient.export(run.id, "markdown"))
          }
        >
          Export portfolio report
        </button>
        {onConversation && (
          <button onClick={() => void onConversation(run.sourceSessionId)}>
            Open project conversation
          </button>
        )}
      </div>
      {error && (
        <p className="task-error" role="alert">
          {error}
        </p>
      )}
      {run.orientation && (
        <details>
          <summary>
            Research standards · {run.orientation.fields.join(", ")}
          </summary>
          <ul>
            {run.orientation.researchStandards.map((s, i) => (
              <li key={i}>{s}</li>
            ))}
          </ul>
          {run.orientation.constraints.length > 0 && (
            <p>{run.orientation.constraints.join(" ")}</p>
          )}
          {run.orientation.assumptions.length > 0 && (
            <p>Assumptions: {run.orientation.assumptions.join(" ")}</p>
          )}
        </details>
      )}
      {run.selection && <p>{run.selection.rationale}</p>}
      {choosing && (
        <div className="discovery-selection">
          <h3>Choose projects to develop</h3>
          <p>
            {selected.length} selected · target {run.definition.paperCount}. You
            can select fewer when the eligible shortlist is smaller.
          </p>
          <button
            className="task-primary"
            disabled={
              busy ||
              !run.selectionHash ||
              !selected.length ||
              selected.length > run.definition.paperCount
            }
            onClick={() =>
              void act(() =>
                discoveryClient.select(
                  run.id,
                  run.selectionHash!,
                  selected,
                  operation.current,
                ),
              )
            }
          >
            Develop selected projects
          </button>
        </div>
      )}
      {!choosing && (
        <button
          aria-expanded={showCandidates}
          onClick={() => setShowCandidates((v) => !v)}
        >
          {showCandidates
            ? "Hide proposals"
            : "Inspect proposals and objections"}
        </button>
      )}
      {(choosing || showCandidates) && (
        <div className="discovery-candidates">
          {choices.map((c) => (
            <article className="discovery-candidate" key={c.id}>
              {choosing ? (
                <label className="mission-check">
                  <input
                    type="checkbox"
                    checked={selected.includes(c.id)}
                    disabled={
                      busy ||
                      (!selected.includes(c.id) &&
                        selected.length >= run.definition.paperCount)
                    }
                    onChange={() => {
                      setSelected((s) =>
                        s.includes(c.id)
                          ? s.filter((id) => id !== c.id)
                          : [...s, c.id],
                      );
                      operation.current = crypto.randomUUID();
                    }}
                  />
                  <strong>
                    {c.id}. {c.proposal.title}
                  </strong>
                </label>
              ) : (
                <h3>
                  {c.id}. {c.proposal.title}
                </h3>
              )}
              <p>{c.proposal.question}</p>
              <p>{c.proposal.contribution}</p>
              <details>
                <summary>Research design and adversarial assessments</summary>
                <p>
                  <strong>Method:</strong> {c.proposal.method}
                </p>
                <p>
                  <strong>First decisive investigation:</strong>{" "}
                  {c.proposal.firstTest}
                </p>
                <p>
                  <strong>Required evidence:</strong>{" "}
                  {c.proposal.requiredEvidence.join("; ")}
                </p>
                <p>
                  <strong>Related work:</strong>{" "}
                  {c.proposal.relatedWork.join("; ") ||
                    "Not established from inspected sources"}
                </p>
                <p>
                  <strong>Risks:</strong> {c.proposal.risks.join("; ")}
                </p>
                {c.assessments.map((a, i) => (
                  <div className="discovery-review" key={i}>
                    <strong>
                      {c.previousVersions.length
                        ? `Referee ${i + 1} on revised proposal`
                        : i === 0
                          ? "Initial screen"
                          : `Referee ${i}`}{" "}
                      · {a.eligible ? "Eligible" : "Ineligible"}
                      {a.duplicateOf
                        ? ` · overlaps project ${a.duplicateOf}`
                        : ""}
                    </strong>
                    <p>{a.strongestObjection}</p>
                    <p>{a.resolution}</p>
                    <p className="task-muted">{a.uncertainty}</p>
                  </div>
                ))}
                {c.previousVersions.length > 0 && (
                  <details>
                    <summary>Earlier proposal versions and objections</summary>
                    {c.previousVersions.map((v, i) => (
                      <article key={i}>
                        <h4>
                          Version {i + 1}: {v.proposal.title}
                        </h4>
                        <p>{v.proposal.method}</p>
                        {v.assessments.map((a, j) => (
                          <p key={j}>
                            {a.strongestObjection} {a.resolution}
                          </p>
                        ))}
                      </article>
                    ))}
                  </details>
                )}
              </details>
            </article>
          ))}
        </div>
      )}
      {!!run.papers.length && (
        <section aria-label="Research papers">
          <h3>Papers</h3>
          {[...run.papers]
            .sort(
              (a, b) =>
                (run.ranking.find((r) => r.candidateId === a.candidateId)
                  ?.rank ?? 100) -
                (run.ranking.find((r) => r.candidateId === b.candidateId)
                  ?.rank ?? 100),
            )
            .map((p) => {
              const rank = run.ranking.find(
                (r) => r.candidateId === p.candidateId,
              );
              return (
                <article className="discovery-candidate" key={p.candidateId}>
                  <h4>
                    {rank ? `${rank.rank}. ` : ""}
                    {p.title || `Project ${p.candidateId}`}
                  </h4>
                  <p>
                    {p.state} · {p.rounds} investigations · {p.versions}{" "}
                    manuscript versions · {p.reviewCount ?? 0} final-version
                    reviews
                  </p>
                  <p>{rank?.reason || p.reason}</p>
                  {rank && <p className="task-muted">{rank.uncertainty}</p>}
                  <button
                    onClick={() => {
                      const request = ++paperRequest.current;
                      setError("");
                      void discoveryClient
                        .paper(run.id, p.candidateId)
                        .then((paper) => {
                          if (request === paperRequest.current) setPaper(paper);
                        })
                        .catch((e) => {
                          if (request === paperRequest.current)
                            setError(String(e));
                        });
                    }}
                  >
                    Read paper and reviews
                  </button>
                </article>
              );
            })}
        </section>
      )}
      {paper && (
        <section className="discovery-paper" aria-label="Paper reader">
          <div className="task-section-heading">
            <h3>
              {current?.manuscript.title || `Project ${paper.candidateId}`}
            </h3>
            <button
              onClick={() => {
                paperRequest.current++;
                setPaper(null);
              }}
              aria-label="Close paper reader"
            >
              ×
            </button>
          </div>
          {current ? (
            <>
              <p>{current.manuscript.abstractText}</p>
              <p className="task-muted">
                Version {current.version} · {current.hash} ·{" "}
                {current.reviews.length
                  ? `${current.reviews.length} adversarial reviews`
                  : "Not adversarially reviewed"}
              </p>
              <ResearchReaderBoundary>
                <Suspense fallback={<p>Opening paper…</p>}>
                  <ReportViewer
                    markdown={current.manuscript.markdown}
                    initialContentsOpen={false}
                  />
                </Suspense>
              </ResearchReaderBoundary>
              <details>
                <summary>Referee reports and response</summary>
                {current.reviews.map((r, i) => (
                  <article key={i}>
                    <h4>Referee {i + 1}</h4>
                    <p>{r.summary}</p>
                    {r.findings.map((f, j) => (
                      <div key={j}>
                        <strong>
                          {f.severity}: {f.claim}
                        </strong>
                        <p>{f.evidence}</p>
                        <p>{f.resolution}</p>
                      </div>
                    ))}
                  </article>
                ))}
                <ul>
                  {current.manuscript.responseToReview.map((s, i) => (
                    <li key={i}>{s}</li>
                  ))}
                </ul>
              </details>
              <details>
                <summary>Earlier manuscript versions</summary>
                {paper.versions.slice(0, -1).map((v) => (
                  <details key={v.version}>
                    <summary>
                      Version {v.version} · {v.reviews.length} reviews
                    </summary>
                    <pre className="discovery-manuscript">
                      {v.manuscript.markdown}
                    </pre>
                  </details>
                ))}
              </details>
            </>
          ) : (
            <p>{paper.reason || "Research is still in progress."}</p>
          )}
          <details>
            <summary>Research and evidence record</summary>
            <pre className="discovery-manuscript">
              {JSON.stringify(
                {
                  rounds: paper.rounds,
                  challenges: paper.challenges,
                  evidence: paper.evidence,
                },
                null,
                2,
              )}
            </pre>
          </details>
        </section>
      )}
    </section>
  );
}
