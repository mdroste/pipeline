import { useState } from "react";
import type { Campaign, CampaignStatus } from "../../lib/programClient";
import type { DeskRecord, OpenResearchObject } from "../../lib/deskClient";
import type { ReviewHandoff } from "../../lib/workbenchTypes";
import {
  button,
  input,
  card,
  muted,
  Field,
  Inspect,
  ObjectSelect,
  Sources,
  operation,
  exactRecord,
  key,
  useProgram,
  type DeskProps,
} from "./shared";
export default function Campaigns(
  props: DeskProps & { onReviewHandoff?: (h: ReviewHandoff) => void },
) {
  const p = useProgram(props, ["revision_campaign"]);
  const [title, setTitle] = useState("Revision round"),
    [round, setRound] = useState(1),
    [manuscript, setManuscript] = useState<OpenResearchObject | null>(null),
    [comments, setComments] = useState<Campaign["comments"]>([]),
    [summary, setSummary] = useState(""),
    [scope, setScope] = useState(""),
    [broad, setBroad] = useState(false),
    [previous, setPrevious] = useState<string | null>(null),
    [supersedes, setSupersedes] = useState<string | null>(null),
    [status, setStatus] = useState<CampaignStatus | null>(null),
    [anchors, setAnchors] = useState<OpenResearchObject[]>([]),
    [dependencies, setDependencies] = useState<OpenResearchObject[]>([]);
  const campaigns = (p.records.revision_campaign ??
    []) as DeskRecord<Campaign>[];
  const choices = p.choices?.objects ?? [];
  const load = (r: DeskRecord<Campaign>) => {
    setTitle(r.title);
    setRound(r.body.round);
    setManuscript(r.body.manuscript);
    setComments(r.body.comments);
    setSummary(r.body.changeSummary);
    setScope(r.body.reviewScope);
    setBroad(r.body.broadChange);
    setPrevious(r.body.previousRound);
    setSupersedes(r.id);
  };
  return (
    <div className="space-y-5">
      <section className={card}>
        <h2 className="font-semibold">Revision campaigns</h2>
        <p className={muted}>
          Group an exact manuscript, original comments, response revisions, and
          required evidence. Save a new version as work progresses; earlier
          rounds remain readable.
        </p>
        <div className="grid grid-cols-2 gap-3">
          <Field label="Campaign title">
            <input
              className={input}
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
          </Field>
          <Field label="Submission round">
            <input
              className={input}
              type="number"
              min="1"
              max="100"
              value={round}
              onChange={(e) => setRound(Number(e.target.value))}
            />
          </Field>
        </div>
        <ObjectSelect
          choices={choices}
          value={manuscript}
          onChange={setManuscript}
          label="Manuscript revision for this round"
          filter={(o) => o.reference.kind === "paper"}
        />
        <Field label="Previous round">
          <select
            className={input}
            value={previous ?? ""}
            onChange={(e) => setPrevious(e.target.value || null)}
          >
            <option value="">First round</option>
            {campaigns
              .filter((c) => c.body.round < round)
              .map((c) => (
                <option value={c.id} key={c.id}>
                  {c.title} · round {c.body.round}
                </option>
              ))}
          </select>
        </Field>
        <Field label="Add original comment and current response">
          <select
            className={input}
            value=""
            onChange={(e) => {
              const r = p.choices?.responses.find(
                (c) => c.id === e.target.value,
              );
              if (r && !comments.some((c) => c.response.id === r.id))
                setComments((old) => [
                  ...old,
                  {
                    response: exactRecord(r),
                    requiredOutputs: [],
                    researcherAddressed: false,
                    judgment: "",
                  },
                ]);
            }}
          >
            <option value="">Choose imported comment</option>
            {p.choices?.responses.map((r) => (
              <option value={r.id} key={r.id}>
                {r.body.decision.number} · {r.body.decision.disposition} ·{" "}
                {r.body.decision.intendedResponse.slice(0, 70)}
              </option>
            ))}
          </select>
        </Field>
        {comments.map((c, i) => (
          <div className="space-y-2 rounded border p-3" key={key(c.response)}>
            <div className="flex gap-2">
              <button
                className={button}
                onClick={() => props.onOpen(c.response)}
              >
                Comment {i + 1} · exact response
              </button>
              <button
                className={button}
                onClick={() =>
                  setComments((old) => old.filter((_, j) => j !== i))
                }
              >
                Remove from round
              </button>
            </div>
            <p className={muted}>Required outputs and evidence</p>
            <Sources
              sources={c.requiredOutputs}
              choices={choices}
              onOpen={props.onOpen}
              onChange={(requiredOutputs) =>
                setComments((old) =>
                  old.map((v, j) => (j === i ? { ...v, requiredOutputs } : v)),
                )
              }
            />
            <label className="text-xs">
              <input
                type="checkbox"
                checked={c.researcherAddressed}
                onChange={(e) =>
                  setComments((old) =>
                    old.map((v, j) =>
                      j === i
                        ? { ...v, researcherAddressed: e.target.checked }
                        : v,
                    ),
                  )
                }
              />{" "}
              I judge this comment addressed
            </label>
            <Field label="Reason for that judgment">
              <textarea
                className={input}
                value={c.judgment}
                onChange={(e) =>
                  setComments((old) =>
                    old.map((v, j) =>
                      j === i ? { ...v, judgment: e.target.value } : v,
                    ),
                  )
                }
              />
            </Field>
          </div>
        ))}
        <Field label="Change summary">
          <textarea
            className={input}
            value={summary}
            onChange={(e) => setSummary(e.target.value)}
          />
        </Field>
        <Field label="Proposed re-review scope and omissions">
          <textarea
            className={input}
            value={scope}
            onChange={(e) => setScope(e.target.value)}
          />
        </Field>
        <label className="text-xs">
          <input
            type="checkbox"
            checked={broad}
            onChange={(e) => setBroad(e.target.checked)}
          />{" "}
          This round changes the argument broadly; a full-manuscript review is
          appropriate
        </label>
        <button
          className={button}
          disabled={
            p.busy || !manuscript || !comments.length || !summary || !scope
          }
          onClick={() =>
            void p.run(async () => {
              const r = await p.call<DeskRecord<Campaign>>({
                action: "saveCampaign",
                title,
                campaign: {
                  round,
                  manuscript,
                  comments,
                  changeSummary: summary,
                  reviewScope: scope,
                  broadChange: broad,
                  previousRound: previous,
                },
                supersedes,
                operationId: operation(),
              });
              setStatus(await p.call({ action: "campaignStatus", id: r.id }));
              setSupersedes(r.id);
            })
          }
        >
          Save round snapshot
        </button>
      </section>
      {campaigns.map((r) => (
        <div className={card} key={r.id}>
          <h3>
            {r.title} · round {r.body.round}
          </h3>
          <button
            className={button}
            onClick={() =>
              void p.run(async () =>
                setStatus(await p.call({ action: "campaignStatus", id: r.id })),
              )
            }
          >
            Check current evidence
          </button>
          <button className={button} onClick={() => load(r)}>
            Revise this snapshot
          </button>
          <button
            className={button}
            onClick={() => {
              load(r);
              setRound(r.body.round + 1);
              setPrevious(r.id);
              setSupersedes(null);
              setComments([]);
              setTitle(`Revision round ${r.body.round + 1}`);
            }}
          >
            Start next round
          </button>
        </div>
      ))}
      {status && (
        <section className={card}>
          <h3 className="font-semibold">
            {status.record.title}: completion evidence
          </h3>
          <p className={muted}>{status.notice}</p>
          {status.comments.map((c) => (
            <div className="space-y-2 border-t pt-3" key={key(c.response)}>
              <h4>
                Comment {c.number} · {c.disposition}
              </h4>
              <p className="whitespace-pre-wrap text-sm">{c.draft}</p>
              <div className="grid grid-cols-2 gap-2 text-xs">
                {[
                  ["Task marked done", c.taskDone],
                  ["Accepted files still current", c.fileAcceptedAndCurrent],
                  ["Check executed", c.checkExecuted],
                  ["Required links pass checks", c.linkChecksPass],
                  ["Researcher judges addressed", c.researcherAddressed],
                ].map(([label, done]) => (
                  <p key={String(label)}>
                    {label}: {done ? "yes" : "no / not linked"}
                  </p>
                ))}
              </div>
              {c.flags.map((f) => (
                <p className="text-xs text-amber-700" key={f}>
                  {f}
                </p>
              ))}
              <p className={muted}>{c.judgment}</p>
              <Inspect
                value={c.requiredOutputs}
                label="Required output freshness"
              />
            </div>
          ))}
          <p className="text-sm">{status.reviewRecommendation}</p>
          <details className="space-y-3">
            <summary>Preview scoped re-review</summary>
            <p className={muted}>
              Selected changed passages must come from this round's manuscript.
              Dependencies may add exact passages from other retained documents.
              The next screen is the ordinary Review launch preview.
            </p>
            <Sources
              sources={anchors}
              choices={choices.filter((o) => o.title === "anchor")}
              onChange={setAnchors}
              onOpen={props.onOpen}
            />
            <p className={muted}>Declared dependency passages</p>
            <Sources
              sources={dependencies}
              choices={choices.filter((o) => o.title === "anchor")}
              onChange={setDependencies}
              onOpen={props.onOpen}
            />
            <button
              className={button}
              disabled={p.busy || !anchors.length || !props.onReviewHandoff}
              onClick={() =>
                void p.run(async () => {
                  const h = await p.call<ReviewHandoff>({
                    action: "campaignReview",
                    id: status.record.id,
                    anchors: anchors.map((a) => a.id),
                    dependencies: dependencies.map((a) => a.id),
                  });
                  props.onReviewHandoff?.(h);
                })
              }
            >
              Prepare paper review
            </button>
          </details>
        </section>
      )}
    </div>
  );
}
