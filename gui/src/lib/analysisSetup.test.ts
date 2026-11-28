import { describe, expect, it } from "vitest";
import {
  declaredFileHashes,
  scriptAdapter,
  scriptSetup,
} from "./analysisSetup";

describe("script setup and recorded output identity", () => {
  it("keeps spaced paths as one argument and notebooks separate from their output", () => {
    const setup = scriptSetup("analysis/my model.ipynb", "windows");
    expect(setup.argv.at(-1)).toBe("analysis/my model.ipynb");
    expect(setup.outputs).toEqual(["analysis/my model.executed.ipynb"]);
    expect(scriptSetup("src/main.do", "windows").outputs).toEqual(["main.log"]);
    expect(scriptAdapter("data.csv")).toBeNull();
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
