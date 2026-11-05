import { describe, expect, it } from "vitest";
import { ARGV_TEXT_HELP, parseArgvText, readArgvText } from "./argvText";

describe("command argument text", () => {
  it("keeps Windows paths typed with single backslashes", () => {
    expect(
      readArgvText(
        String.raw`["C:\Program Files\Stata18\StataMP-64.exe", "/e", "do", "main.do"]`,
      ),
    ).toEqual([
      String.raw`C:\Program Files\Stata18\StataMP-64.exe`,
      "/e",
      "do",
      "main.do",
    ]);
  });

  it("does not turn \\t, \\n, \\r, \\b, or \\f in a path into control characters", () => {
    expect(
      readArgvText(String.raw`["D:\tools\new.do", "C:\research\raw\b\f.csv"]`),
    ).toEqual([
      String.raw`D:\tools\new.do`,
      String.raw`C:\research\raw\b\f.csv`,
    ]);
  });

  it("reads what JSON.stringify writes for paths and quotes", () => {
    const argv = [
      String.raw`C:\Program Files\R\R-4.4.1\bin\Rscript.exe`,
      String.raw`\\server\share\analysis.R`,
      'say "hi"',
      "/usr/bin/env",
    ];
    expect(readArgvText(JSON.stringify(argv))).toEqual(argv);
    expect(readArgvText(JSON.stringify(argv, null, 2))).toEqual(argv);
  });

  it("keeps UNC paths and a trailing backslash typed as is", () => {
    expect(
      readArgvText(String.raw`["robocopy", "\\server\share\in", "C:\out\" ]`),
    ).toEqual(["robocopy", String.raw`\\server\share\in`, "C:\\out\\"]);
  });

  it("keeps an escaped quote inside an argument that also holds a path", () => {
    expect(
      readArgvText(String.raw`["python", "-c", "open(\"C:\data\y.csv\")"]`),
    ).toEqual(["python", "-c", String.raw`open("C:\data\y.csv")`]);
  });

  it("rejects text that is not a list of quoted values", () => {
    for (const text of [
      "python main.py",
      '["python", 3]',
      '["python"',
      "{}",
      '["a"] x',
    ]) {
      expect(readArgvText(text)).toBeNull();
    }
    expect(() => parseArgvText("python main.py")).toThrow(ARGV_TEXT_HELP);
    expect(readArgvText("[]")).toEqual([]);
  });
});
