import type { DatasetVersion, SourceFormat } from "../../lib/deskClient";
import ConnectorNotice, { datasetConnector } from "./ConnectorNotice";
import { muted } from "./shared";

/** `.` 2 · `.a` 1 — missing codes are kept distinct and never shown as values. */
export function missingSummary(
  missing: number,
  codes: Record<string, number> | undefined,
): string {
  const entries = Object.entries(codes ?? {});
  if (!entries.length) return String(missing);
  const detail = entries.map(([code, n]) => `${code} ${n}`).join(" · ");
  return `${missing} (${detail})`;
}

function formatLine(f: SourceFormat | null | undefined): string | null {
  if (!f) return null;
  if (f.kind === "stata")
    return [
      `Stata release ${f.release ?? "unknown"}`,
      f.label ? `“${f.label}”` : null,
      f.timestamp ? `saved ${f.timestamp}` : null,
    ]
      .filter(Boolean)
      .join(" · ");
  if (f.kind === "xlsx")
    return [
      `Excel sheet ${f.sheet ?? "1"}`,
      f.sheets && f.sheets.length > 1
        ? `${f.sheets.length} sheets in the workbook`
        : null,
      f.dateSystem ? `${f.dateSystem} date system` : null,
    ]
      .filter(Boolean)
      .join(" · ");
  return f.kind;
}

/** Variable dictionary with declared labels, formats, value labels and
 * missing codes, plus a connector's terms and required notice. */
export default function DatasetDictionary({
  version,
}: {
  version: DatasetVersion;
}) {
  const connector = datasetConnector(version);
  const sets = version.valueLabels ?? [];
  const format = formatLine(version.sourceFormat);
  const declared = version.columns.some(
    (c) => c.label || c.storageType || c.displayFormat || c.valueLabels,
  );
  return (
    <div className="space-y-2">
      {format && <p className={muted}>{format}</p>}
      <div className="max-h-64 overflow-auto">
        <table className="w-full text-left text-xs">
          <thead>
            <tr>
              <th>Variable</th>
              {declared && <th>Label</th>}
              <th>Type</th>
              {declared && <th>Format</th>}
              {declared && <th>Value labels</th>}
              <th>Missing</th>
              <th>Units</th>
            </tr>
          </thead>
          <tbody>
            {version.columns.map((c) => (
              <tr key={c.name}>
                <td>{c.name}</td>
                {declared && <td>{c.label ?? ""}</td>}
                <td>{c.storageType ?? c.inferredType}</td>
                {declared && <td>{c.displayFormat ?? ""}</td>}
                {declared && <td>{c.valueLabels ?? ""}</td>}
                <td>{missingSummary(c.missing, c.missingCodes)}</td>
                <td>{c.units ?? "Undeclared"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {sets.length > 0 && (
        <details>
          <summary className="text-xs font-semibold">
            Value labels ({sets.length} {sets.length === 1 ? "set" : "sets"})
          </summary>
          {sets.map((set) => (
            <div key={set.name} className="mt-2">
              <p className="text-xs font-semibold">{set.name}</p>
              <ul className="text-xs">
                {set.labels.map((l) => (
                  <li key={l.value}>
                    {l.value} = {l.label}
                  </li>
                ))}
              </ul>
              {!!set.omitted && (
                <p className={muted}>
                  {set.omitted} more labels were not catalogued.
                </p>
              )}
            </div>
          ))}
        </details>
      )}
      {version.connector && (
        <details>
          <summary className="text-xs font-semibold">
            Source terms and citation
          </summary>
          <dl className="mt-2 space-y-1 text-xs">
            <dt className="font-semibold">Licence</dt>
            <dd>{version.connector.licence}</dd>
            <dt className="font-semibold">Suggested citation</dt>
            <dd>{version.connector.citation}</dd>
            {version.connector.lastUpdated && (
              <>
                <dt className="font-semibold">Source last updated</dt>
                <dd>{version.connector.lastUpdated}</dd>
              </>
            )}
            <dt className="font-semibold">Terms last reviewed by Pipeline</dt>
            <dd>{version.connector.termsReviewed}</dd>
          </dl>
        </details>
      )}
      {connector && (
        <ConnectorNotice
          connector={connector}
          recorded={version.connector?.notice}
        />
      )}
    </div>
  );
}
