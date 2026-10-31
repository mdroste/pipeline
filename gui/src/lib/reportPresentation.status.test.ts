import { describe, expect, it } from "vitest";
import { listedRunStatus, reportStatusDisplay } from "./reportPresentation";
import type { ReportQuality } from "./types";

const quality = (overrides: Partial<ReportQuality>) =>
  ({
    extraction_notes: [],
    partially_verified_evidence: 0,
    unverified_evidence: 0,
    manual_review_items: 0,
    ...overrides,
  }) as ReportQuality;

describe("reportStatusDisplay", () => {
  it("does not call a report limited because of one comment's quotation", () => {
    expect(
      reportStatusDisplay(
        "degraded",
        quality({ partially_verified_evidence: 1, unverified_evidence: 1 }),
      ),
    ).toEqual({
      label: "Completed",
      heading: "Completed report",
      tone: "good",
      notes: ["2 quotes need checking"],
    });
  });

  it("says nothing extra after a retry that succeeded", () => {
    const display = reportStatusDisplay("degraded", quality({}));
    expect(display.heading).toBe("Completed report");
    expect(display.notes).toEqual([]);
  });

  it("ignores withheld image text, as the stored status does", () => {
    const display = reportStatusDisplay(
      "degraded",
      quality({
        extraction_notes: [
          "Image text withheld as unverified: 1 image region(s).",
        ],
        unverified_evidence: 1,
        manual_review_items: 1,
      }),
    );
    expect(display.heading).toBe("Completed report");
    expect(display.notes).toEqual([
      "1 quote needs checking",
      "1 comment needs manual review",
    ]);
  });

  it("announces a limitation when the document may not have been read fully", () => {
    expect(
      reportStatusDisplay(
        "degraded",
        quality({
          extraction_notes: ["Page 45: recovered incomplete layout."],
          unverified_evidence: 1,
        }),
      ),
    ).toEqual({
      label: "Completed with limitations",
      heading: "Completed with limitations",
      tone: "caution",
      notes: [
        "Part of the document may be incomplete",
        "1 quote needs checking",
      ],
    });
  });

  it("assumes a limitation when the quality record is missing", () => {
    const display = reportStatusDisplay("degraded");
    expect(display.heading).toBe("Completed with limitations");
    expect(display.notes).toEqual([]);
  });

  it("keeps the other statuses", () => {
    expect(reportStatusDisplay("done").heading).toBe("Completed report");
    expect(reportStatusDisplay("partial")).toMatchObject({
      heading: "partial report",
      tone: "caution",
    });
    expect(reportStatusDisplay("cancelled").tone).toBe("stopped");
    expect(reportStatusDisplay("failed").tone).toBe("bad");
  });
});

it("lists a degraded run as done", () => {
  expect(listedRunStatus("degraded")).toBe("done");
  expect(listedRunStatus("partial")).toBe("partial");
});
