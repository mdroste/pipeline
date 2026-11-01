import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { workbenchErrorMessage } from "./workbenchError";

describe("workbenchErrorMessage", () => {
  it("renders structured Tauri command errors with their recovery guidance", () => {
    expect(
      workbenchErrorMessage({
        code: "app_server_unavailable",
        message: "No compatible Codex runtime was found.",
        retryable: false,
        recovery: "Install Codex CLI 0.147.0 or newer and try again.",
      }),
    ).toBe(
      "No compatible Codex runtime was found. Install Codex CLI 0.147.0 or newer and try again.",
    );
  });

  it("renders ordinary JavaScript and string errors", () => {
    expect(workbenchErrorMessage(new Error("The request timed out."))).toBe(
      "The request timed out.",
    );
    expect(workbenchErrorMessage("Connection closed")).toBe(
      "Connection closed",
    );
  });

  it("does not pass on text left behind by coercing an object", () => {
    expect(workbenchErrorMessage("[object Object]")).toBe(
      "An unexpected project error occurred.",
    );
    expect(workbenchErrorMessage(new Error("[object Object]"))).toBe(
      "An unexpected project error occurred.",
    );
  });

  it("never exposes an object coercion placeholder", () => {
    expect(workbenchErrorMessage({ code: "unknown" })).toBe(
      "An unexpected project error occurred.",
    );
  });
});

describe("project views", () => {
  // Workbench commands reject with objects, and a catch parameter is untyped,
  // so the compiler can't see String(e) turn one into "[object Object]".
  it("format caught errors instead of coercing them", () => {
    const root = join(process.cwd(), "src/components");
    const coerced = [
      "project-surface",
      "research-desk",
      "research-panel",
      "research-programs",
      "research-studio",
    ].flatMap((folder) =>
      readdirSync(join(root, folder))
        .filter((file) => /\.tsx?$/.test(file) && !file.includes(".test."))
        .filter((file) =>
          /\bString\((e|err|error|cause|caught)\)|\.catch\(on[A-Z]\w*\)/.test(
            readFileSync(join(root, folder, file), "utf8"),
          ),
        )
        .map((file) => `${folder}/${file}`),
    );
    expect(coerced).toEqual([]);
  });
});
