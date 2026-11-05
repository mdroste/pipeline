import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { Sources } from "./shared";
import type { ObjectChoice } from "../../lib/programClient";

vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));

const anchor = (id: string, quote: string): ObjectChoice => ({
  reference: { kind: "record", id, revision: "1" },
  title: "anchor",
  label: quote,
  recordKind: "anchor",
  provenance: "project record",
});
afterEach(cleanup);

it("tells saved passages apart by their quotes", () => {
  const first = anchor("a1", "Wages rise with tenure.");
  render(
    <Sources
      sources={[first.reference]}
      choices={[first, anchor("a2", "Hours fall after the reform.")]}
      onChange={vi.fn()}
      onOpen={vi.fn()}
    />,
  );
  expect(
    screen.getByRole("option", {
      name: "Wages rise with tenure. · anchor · 1",
    }),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("option", {
      name: "Hours fall after the reform. · anchor · 1",
    }),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: /^Wages rise with tenure\. · 1/ }),
  ).toBeInTheDocument();
});
