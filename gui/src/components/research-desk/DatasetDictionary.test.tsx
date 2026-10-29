import { render, screen, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import type { DatasetVersion } from "../../lib/deskClient";
import DatasetDictionary, { missingSummary } from "./DatasetDictionary";

vi.mock("@tauri-apps/plugin-shell", () => ({ open: vi.fn() }));

const acquisition = {
  provider: "local import",
  source: "survey.dta",
  retrievedAt: "2026-10-28T00:00:00Z",
  requestedVintage: null,
  returnedVintage: null,
  seriesIds: [],
  units: null,
  frequency: null,
  transformation: null,
};
const base: DatasetVersion = {
  schemaVersion: 1,
  artifactId: "a",
  contentHash: "h".repeat(64),
  externalReference: null,
  rows: 4,
  columns: [],
  acquisition,
  diagnosticsCoverage: "exhaustive captured rows",
  delimiter: "csv",
  limitations: [],
};

it("shows Stata labels, formats, value labels and distinct missing codes", () => {
  render(
    <DatasetDictionary
      version={{
        ...base,
        delimiter: "dta",
        sourceFormat: {
          kind: "stata",
          release: "118",
          label: "Household survey",
        },
        columns: [
          {
            name: "sex",
            inferredType: "number",
            missing: 2,
            units: null,
            description: null,
            origin: "declared in the Stata file",
            label: "Sex of respondent",
            storageType: "byte",
            displayFormat: "%8.0g",
            valueLabels: "sexlbl",
            missingCodes: { ".": 1, ".z": 1 },
          },
        ],
        valueLabels: [
          {
            name: "sexlbl",
            labels: [
              { value: "1", label: "Male" },
              { value: ".a", label: "Refused" },
            ],
            omitted: 3,
          },
        ],
      }}
    />,
  );
  expect(
    screen.getByText("Stata release 118 · “Household survey”"),
  ).toBeInTheDocument();
  const row = screen.getByRole("row", { name: /sex/ });
  for (const text of ["Sex of respondent", "byte", "%8.0g", "sexlbl"])
    expect(within(row).getByText(text)).toBeInTheDocument();
  expect(within(row).getByText("2 (. 1 · .z 1)")).toBeInTheDocument();
  expect(screen.getByText("Value labels (1 set)")).toBeInTheDocument();
  expect(screen.getByText(".a = Refused")).toBeInTheDocument();
  expect(
    screen.getByText("3 more labels were not catalogued."),
  ).toBeInTheDocument();
  expect(screen.queryByRole("note")).not.toBeInTheDocument();
});

it("keeps the CSV layout and shows the FRED notice for earlier FRED captures", () => {
  render(
    <DatasetDictionary
      version={{
        ...base,
        acquisition: { ...acquisition, provider: "FRED/ALFRED" },
        columns: [
          {
            name: "value",
            inferredType: "number",
            missing: 1,
            units: null,
            description: null,
            origin: "inferred from all captured rows",
          },
        ],
      }}
    />,
  );
  expect(screen.queryByText("Label")).not.toBeInTheDocument();
  expect(screen.getByRole("note", { name: "FRED® notice" })).toHaveTextContent(
    "not endorsed or certified by the Federal Reserve Bank of St. Louis",
  );
});

it("shows recorded source terms, citation and provider-specific notices", () => {
  render(
    <DatasetDictionary
      version={{
        ...base,
        connector: {
          connector: "dbnomics",
          query: { seriesId: "IMF/WEO:2024-10/USA.NGDP_RPCH.pcent_change" },
          requestUrls: [],
          lastUpdated: "2024-10-31T02:17:18.509Z",
          licence: "Open Database License (ODbL) 1.0",
          notice:
            "Data retrieved through DBnomics (Cepremap). Source: International Monetary Fund.",
          citation: "International Monetary Fund, World Economic Outlook.",
          termsUrl: "https://docs.db.nomics.world/legal-terms",
          termsReviewed: "2026-10-28",
        },
      }}
    />,
  );
  expect(
    screen.getByText("International Monetary Fund, World Economic Outlook."),
  ).toBeInTheDocument();
  expect(screen.getByText("2026-10-28")).toBeInTheDocument();
  expect(
    screen.getByRole("note", { name: "DBnomics notice" }),
  ).toHaveTextContent("Source: International Monetary Fund.");
});

it("summarizes missing counts without codes for CSV", () => {
  expect(missingSummary(3, undefined)).toBe("3");
  expect(missingSummary(2, { "#N/A": 1, empty: 1 })).toBe(
    "2 (#N/A 1 · empty 1)",
  );
});
