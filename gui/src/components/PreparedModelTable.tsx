import type { RosterEntry } from "../lib/types";
import { PROVIDER_LABELS, type Provider } from "../lib/providers";
import { rosterSourceLabel } from "../lib/modelChoice";

/** Render the immutable launch plan, never the current global settings. */
export default function PreparedModelTable({
  entries,
}: {
  entries?: RosterEntry[];
}) {
  return (
    <section aria-label="Models in this review" className="mt-6">
      <h2 className="text-sm font-semibold text-ink">Models in this review</h2>
      {entries ? (
        <div className="mt-2 overflow-x-auto rounded-lg border border-line">
          <table className="w-full text-left text-xs">
            <thead className="bg-sunken text-ink-muted">
              <tr>
                {["Step", "Provider / model", "Thinking", "Set by"].map(
                  (label) => (
                    <th
                      key={label}
                      scope="col"
                      className="px-3 py-2 font-medium"
                    >
                      {label}
                    </th>
                  ),
                )}
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {entries.map((entry, index) => (
                <tr key={index}>
                  <th scope="row" className="px-3 py-2 font-medium">
                    {entry.step_labels.length
                      ? entry.step_labels.join(", ")
                      : entry.label}
                    {entry.note && (
                      <span className="mt-1 block font-normal text-ink-muted">
                        {entry.note}
                      </span>
                    )}
                  </th>
                  <td className="px-3 py-2">
                    {PROVIDER_LABELS[entry.provider as Provider] ??
                      entry.provider}{" "}
                    ·{" "}
                    {entry.provider === "local"
                      ? "Local server"
                      : entry.transport === "api"
                        ? "API connection"
                        : "Account connection"}
                    <br />
                    {entry.model.mode === "pinned"
                      ? entry.model.model
                      : "Automatic — chosen at launch"}
                  </td>
                  <td className="px-3 py-2">
                    {entry.effort || "Provider default"}
                  </td>
                  <td className="px-3 py-2">{rosterSourceLabel(entry)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {entries.length === 0 && (
            <p className="p-3 text-xs">This plan has no model calls.</p>
          )}
        </div>
      ) : (
        <p role="status" className="mt-2 text-xs text-ink-muted">
          Model details are unavailable in this preview. Go back and preview
          again to check them.
        </p>
      )}
    </section>
  );
}
