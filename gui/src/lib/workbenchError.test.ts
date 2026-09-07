import { describe, expect, it } from "vitest";
import { workbenchErrorMessage } from "./workbenchError";

describe("workbenchErrorMessage", () => {
  it("renders structured Tauri command errors with their recovery guidance", () => {
    expect(workbenchErrorMessage({
      code: "app_server_unavailable",
      message: "No compatible Codex runtime was found.",
      retryable: false,
      recovery: "Install Codex CLI 0.147.0 or newer and try again.",
    })).toBe(
      "No compatible Codex runtime was found. Install Codex CLI 0.147.0 or newer and try again.",
    );
  });

  it("renders ordinary JavaScript and string errors", () => {
    expect(workbenchErrorMessage(new Error("The request timed out."))).toBe(
      "The request timed out.",
    );
    expect(workbenchErrorMessage("Connection closed")).toBe("Connection closed");
  });

  it("never exposes an object coercion placeholder", () => {
    expect(workbenchErrorMessage({ code: "unknown" })).toBe(
      "An unexpected Workspace error occurred.",
    );
  });
});
