import { describe, expect, it } from "vitest";
import {
  declaredFileHashes,
  scriptAdapter,
  scriptSetup,
} from "./analysisSetup";
import { fileRunLinks } from "../components/research-panel/FileRuns";
import type { ResearchExecution } from "./workbench/executionTypes";

describe("script setup and recorded output identity", () => {
  it("keeps spaced paths as one argument and notebooks separate from their output", () => {
    const setup = scriptSetup("analysis/my model.ipynb", "windows");
    expect(setup.argv.at(-1)).toBe("analysis/my model.ipynb");
    expect(setup.outputs).toEqual(["analysis/my model.executed.ipynb"]);
    expect(scriptSetup("src/main.do", "windows").outputs).toEqual(["main.log"]);
    expect(scriptAdapter("data.csv")).toBeNull();
  });
  it("does not infer provenance from a filename or timestamp alone", () => {
    const run = {
      cwd: "C:\\research",
      outputManifest: {
        artifacts: [{ path: "table.csv", contentHash: "old" }],
      },
      inputManifest: {},
    } as unknown as ResearchExecution;
    expect(fileRunLinks([run], "C:\\research", "table.csv", "new")).toEqual([]);
    expect(
      fileRunLinks([run], "C:\\research", "table.csv", "old")[0].role,
    ).toBe("Produced this file version");
    expect(fileRunLinks([run], "C:\\other", "table.csv", "old")).toEqual([]);
  });
  it("ignores input ordering but detects changed and unavailable identities", () => {
    const a = { path: "a", contentHash: "one" },
      b = { path: "b", contentHash: "two" };
    expect(declaredFileHashes({ files: [a, b] })).toBe(
      declaredFileHashes({ files: [b, a] }),
    );
    expect(declaredFileHashes({ files: [a] })).not.toBe(
      declaredFileHashes({ files: [{ path: "a", unavailable: "offline" }] }),
    );
  });
});
