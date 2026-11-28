import { describe, expect, it } from "vitest";
import { displayPath } from "./displayPath";

describe("displayPath", () => {
  it("drops the Windows verbatim prefix from drive and UNC paths", () => {
    expect(displayPath("\\\\?\\G:\\GitHub\\jmp")).toBe("G:\\GitHub\\jmp");
    expect(displayPath("\\\\?\\G:")).toBe("G:");
    expect(displayPath("\\\\?\\UNC\\server\\share\\paper")).toBe(
      "\\\\server\\share\\paper",
    );
  });

  it("leaves other paths alone", () => {
    expect(displayPath("G:\\GitHub\\jmp")).toBe("G:\\GitHub\\jmp");
    expect(displayPath("/Users/me/paper")).toBe("/Users/me/paper");
    expect(displayPath("\\\\?\\Volume{abc}\\x")).toBe("\\\\?\\Volume{abc}\\x");
    expect(displayPath(null)).toBeNull();
  });
});
