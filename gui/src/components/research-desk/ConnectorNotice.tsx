import { open as openExternal } from "@tauri-apps/plugin-shell";
import type { ConnectorId, DatasetVersion } from "../../lib/deskClient";
import { linkButton, muted } from "../../ui/classes";

export const FRED_TERMS_URL =
  "https://fred.stlouisfed.org/docs/api/terms_of_use.html";

/** Attribution each statistical source requires wherever its data appear. */
export const CONNECTOR_NOTICES: Record<
  ConnectorId,
  { source: string; text: string[]; link: string; url: string }
> = {
  fred: {
    source: "FRED®",
    text: [
      "This product uses the FRED® API but is not endorsed or certified by the Federal Reserve Bank of St. Louis.",
      "By using FRED® data, you agree to be bound by the FRED® API Terms of Use.",
    ],
    link: "FRED® API Terms of Use",
    url: FRED_TERMS_URL,
  },
  dbnomics: {
    source: "DBnomics",
    text: [
      "Data from DBnomics (Cepremap) are distributed under the Open Database License (ODbL). Each series also keeps the licence and terms of use of its original provider, such as the IMF, OECD or Eurostat.",
    ],
    link: "DBnomics legal terms",
    url: "https://docs.db.nomics.world/legal-terms",
  },
  world_bank: {
    source: "World Bank",
    text: [
      "World Bank data are licensed under Creative Commons Attribution 4.0 (CC BY 4.0) unless an indicator's terms say otherwise. Credit the World Bank and the indicator's source when you use them.",
    ],
    link: "World Bank data licences",
    url: "https://datacatalog.worldbank.org/public-licenses",
  },
};

/** The connector behind a dataset, including FRED captures made before
 * connector terms were recorded. */
export function datasetConnector(version: DatasetVersion): ConnectorId | null {
  if (version.connector?.connector) return version.connector.connector;
  return version.acquisition?.provider === "FRED/ALFRED" ? "fred" : null;
}

export default function ConnectorNotice({
  connector,
  recorded,
}: {
  connector: ConnectorId;
  /** Notice text recorded on the dataset version, when it differs. */
  recorded?: string | null;
}) {
  const notice = CONNECTOR_NOTICES[connector];
  const extra =
    recorded && !notice.text.some((t) => recorded.startsWith(t))
      ? recorded
      : null;
  return (
    <div
      role="note"
      aria-label={`${notice.source} notice`}
      className="space-y-1 rounded-md border border-gray-200 p-3 dark:border-gray-800"
    >
      {notice.text.map((t) => (
        <p key={t} className={muted}>
          {t}
        </p>
      ))}
      {extra && <p className={muted}>{extra}</p>}
      <a
        className={linkButton}
        href={notice.url}
        onClick={(e) => {
          e.preventDefault();
          void openExternal(notice.url).catch(() => undefined);
        }}
      >
        {notice.link}
      </a>
    </div>
  );
}
