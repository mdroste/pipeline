import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";
import { plainLabel } from "./research-panel/format";

const root = join(process.cwd(), "src/components");
const sources = (folder: string): string[] =>
  readdirSync(folder).flatMap((name) => {
    const path = join(folder, name);
    if (statSync(path).isDirectory()) return sources(path);
    return /\.tsx?$/.test(name) && !name.includes(".test.") ? [path] : [];
  });

describe("researcher-facing copy", () => {
  it.each([
    ["research-studio/Responses.tsx", /Workflow/],
    [
      "WorkspaceExchangePanel.tsx",
      /Task change sets|Acceptance journals|recipe runs|blob\(s\)|Draft workflow/,
    ],
    ["WorkspaceReleasePanel.tsx", /choose the workflow/i],
    ["ProjectIssueLedgerPanel.tsx", /All workflows/],
    ["harness-editor/harnessHelpers.ts", /label: "Workspace"/],
    ["harness-editor/HarnessPresetPanel.tsx", /additional Workspace/],
    ["research-studio/Experiments.tsx", /oldstata/],
  ])("keeps implementation terms out of %s", (file, terms) => {
    expect(readFileSync(join(root, file), "utf8")).not.toMatch(terms);
  });

  it.each([
    ["human_confirmed", "Confirmed by you"],
    ["local_pdf", "Local PDF"],
    ["source_tree", "Folder of source files"],
    ["outcome_unknown", "Outcome unknown"],
    ["user", "You"],
    ["under_review", "Under review"],
    ["queued", "Queued"],
  ])("shows the stored value %s as plain words", (stored, shown) => {
    expect(plainLabel(stored)).toBe(shown);
  });
});

describe("warning and error colours", () => {
  // These two shades are too dark for the dark theme. The text-danger and
  // text-warning tokens change with it.
  it("are readable in dark mode", () => {
    const lightOnly = /(?<![:\w-])text-(red|amber)-700\b/;
    const darkPair = /\bdark:text-(red|amber)-/;
    const found = sources(root).flatMap((path) =>
      readFileSync(path, "utf8")
        .split("\n")
        .flatMap((line, index) =>
          lightOnly.test(line) && !darkPair.test(line)
            ? [`${relative(root, path)}:${index + 1}`]
            : [],
        ),
    );
    expect(found).toEqual([]);
  });
});
