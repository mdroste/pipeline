import { useState } from "react";
import { deskClient, type DeskRecord } from "../../lib/deskClient";
import ConnectorNotice from "./ConnectorNotice";
import { button, card, input, muted } from "./shared";

/** Capture forms for statistical sources. Each source's required notice is
 * shown beside its form; captures need the project's download permission. */
export default function DataConnectors({
  workspaceId,
  busy,
  run,
  onError,
}: {
  workspaceId: string;
  busy: boolean;
  run: (fn: () => Promise<unknown>) => Promise<void>;
  onError: (message: string) => void;
}) {
  const [series, setSeries] = useState("");
  const [vintage, setVintage] = useState("");
  const [key, setKey] = useState("");
  const [dbnomics, setDbnomics] = useState("");
  const [indicator, setIndicator] = useState("");
  const [countries, setCountries] = useState("");
  const [dates, setDates] = useState("");
  const capture = (fn: () => Promise<DeskRecord>) =>
    run(async () => {
      const receipt = await fn();
      if (receipt.kind === "acquisition")
        onError(String(receipt.body.error ?? "Acquisition failed"));
    });
  return (
    <>
      <details className={card}>
        <summary className="font-semibold">FRED / ALFRED vintage</summary>
        <input
          aria-label="FRED series ID"
          className={input}
          placeholder="Series ID, e.g. GDP"
          value={series}
          onChange={(e) => setSeries(e.target.value)}
        />
        <input
          aria-label="FRED vintage date"
          className={input}
          placeholder="Vintage date (YYYY-MM-DD)"
          value={vintage}
          onChange={(e) => setVintage(e.target.value)}
        />
        <input
          aria-label="FRED API key"
          type="password"
          autoComplete="off"
          className={input}
          placeholder="API key for this request only"
          value={key}
          onChange={(e) => setKey(e.target.value)}
        />
        <p className={muted}>
          Download the selected vintage in its original units. First allow data
          downloads in Library. Pipeline uses the API key for this request only.
        </p>
        <ConnectorNotice connector="fred" />
        <button
          className={button}
          disabled={busy || !series || !vintage || !key}
          onClick={() => {
            const apiKey = key;
            setKey("");
            void capture(() =>
              deskClient.fred(workspaceId, series, vintage, apiKey),
            );
          }}
        >
          Download this vintage
        </button>
      </details>
      <details className={card}>
        <summary className="font-semibold">DBnomics series</summary>
        <input
          aria-label="DBnomics series ID"
          className={input}
          placeholder="provider/dataset/series, e.g. IMF/WEO:2024-10/USA.NGDP_RPCH.pcent_change"
          value={dbnomics}
          onChange={(e) => setDbnomics(e.target.value)}
        />
        <p className={muted}>
          DBnomics gathers series from the IMF, OECD, Eurostat, the ECB, central
          banks and other providers. Copy the series ID from its page on
          db.nomics.world. No account is needed.
        </p>
        <ConnectorNotice connector="dbnomics" />
        <button
          className={button}
          disabled={busy || !dbnomics.trim()}
          onClick={() =>
            void capture(() =>
              deskClient.series(workspaceId, {
                connector: "dbnomics",
                seriesId: dbnomics.trim(),
              }),
            )
          }
        >
          Download series
        </button>
      </details>
      <details className={card}>
        <summary className="font-semibold">World Bank indicator</summary>
        <input
          aria-label="World Bank indicator code"
          className={input}
          placeholder="Indicator code, e.g. NY.GDP.MKTP.CD"
          value={indicator}
          onChange={(e) => setIndicator(e.target.value)}
        />
        <input
          aria-label="World Bank economies"
          className={input}
          placeholder="Economy codes, e.g. USA;BRA (or all)"
          value={countries}
          onChange={(e) => setCountries(e.target.value)}
        />
        <input
          aria-label="World Bank years"
          className={input}
          placeholder="Optional years, e.g. 2000:2020"
          value={dates}
          onChange={(e) => setDates(e.target.value)}
        />
        <p className={muted}>
          Download one indicator for up to 60 economies (at most 20,000
          observations). No account is needed.
        </p>
        <ConnectorNotice connector="world_bank" />
        <button
          className={button}
          disabled={busy || !indicator.trim() || !countries.trim()}
          onClick={() =>
            void capture(() =>
              deskClient.series(workspaceId, {
                connector: "world_bank",
                indicator: indicator.trim(),
                countries: countries.trim(),
                dateRange: dates.trim() || null,
              }),
            )
          }
        >
          Download indicator
        </button>
      </details>
    </>
  );
}
