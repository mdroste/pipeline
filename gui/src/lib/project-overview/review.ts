import type { OverviewBlock, OverviewRow, ProjectSignals } from "./types";
import type { ProjectHome } from "../projectClient";
import { block, clip, plural } from "./presentation";
import { currentManuscript } from "./manuscript";

export function syncBlock(data: ProjectHome): OverviewBlock | null {
  const rows: OverviewRow[] = [];
  if (
    data.applications.some((item) =>
      ["applying", "recovery_required"].includes(item.body.state),
    )
  )
    rows.push({
      id: "recovery",
      text: "An accepted edit was interrupted",
      attention: true,
      lane: "needs",
      action: { kind: "tab", label: "Inspect edit recovery", tab: "edits" },
    });
  if (currentManuscript(data).superseded)
    rows.push({
      id: "superseded",
      text: "A newer version of the paper was imported, but an earlier one is still marked current",
      attention: true,
      action: {
        kind: "tab",
        label: "Choose the current version",
        tab: "project-settings",
      },
    });
  const changed = data.workingCopyChanged ?? 0;
  if (changed)
    rows.push({
      id: "working-copy",
      text: `${plural(changed, "source file")} changed since the current version was imported`,
      detail: "Only files the paper depends on are compared",
      attention: true,
      action: { kind: "tab", label: "Open documents", tab: "documents" },
    });
  return block("sync", "Out of sync", rows);
}

/** Revision round: progress through review comments. Only when comments exist. */
export function roundBlock(signals: ProjectSignals): OverviewBlock | null {
  const rows: OverviewRow[] = [];
  const responses = signals.responses ?? [];
  if (responses.length) {
    const count = (disposition: string) =>
      responses.filter((r) => r.body.decision.disposition === disposition)
        .length;
    const pending = responses.filter(
      (r) => !["addressed", "rejected"].includes(r.body.decision.disposition),
    );
    rows.push({
      id: "progress",
      text: `${count("addressed")} of ${plural(responses.length, "review comment")} addressed`,
      detail: [
        count("investigating") ? `${count("investigating")} in progress` : "",
        count("open") ? `${count("open")} open` : "",
        count("deferred") ? `${count("deferred")} deferred` : "",
        count("rejected") ? `${count("rejected")} rejected` : "",
      ]
        .filter(Boolean)
        .join(" · "),
      lane: pending.length ? "next" : undefined,
      action: { kind: "tab", label: "Open responses", tab: "responses" },
    });
    const undrafted = pending.filter((r) => !r.body.decision.draft.trim());
    if (undrafted.length)
      rows.push({
        id: "undrafted",
        text: `${plural(undrafted.length, "comment")} ${undrafted.length === 1 ? "has" : "have"} no drafted reply`,
        lane: "next",
        action: {
          kind: "ask",
          label: "Ask the assistant to draft replies",
          prompt: `Draft replies for the review comments in this project that have no drafted reply yet (${undrafted
            .slice(0, 30)
            .map((r) => r.body.decision.number || r.id)
            .join(
              ", ",
            )}). For each, state what the manuscript now says or what analysis would resolve it. Propose the replies for my review; do not mark any comment addressed.`,
        },
      });
    const waiting = pending.filter(
      (r) =>
        r.body.decision.resolvingCheck.trim() && !r.body.decision.executionId,
    );
    if (waiting.length)
      rows.push({
        id: "waiting",
        text: `${plural(waiting.length, "comment")} ${waiting.length === 1 ? "names" : "name"} a check that has not been run`,
        detail: clip(waiting[0].body.decision.resolvingCheck),
        attention: true,
        lane: "next",
        action: { kind: "tab", label: "Open responses", tab: "responses" },
      });
    const flagged = responses.filter((r) => r.body.flags.length);
    if (flagged.length)
      rows.push({
        id: "flagged",
        text: `${plural(flagged.length, "response")} ${flagged.length === 1 ? "is" : "are"} flagged`,
        detail: clip(flagged[0].body.flags[0]),
        attention: true,
        lane: "next",
        action: { kind: "tab", label: "Open responses", tab: "responses" },
      });
  }
  return block("round", "Revision round", rows);
}
