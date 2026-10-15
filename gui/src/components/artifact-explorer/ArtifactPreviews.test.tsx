import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { BinaryCard } from "./ArtifactPreviews";

const openExternal = vi.hoisted(() => vi.fn(() => Promise.resolve()));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: openExternal }));

const content = (kind: string) => ({
  kind,
  bytes: 1024,
  text: null,
  base64: null,
  truncated: false,
  abs_path: "/runs/x/artifacts/summary.jar",
});

describe("BinaryCard", () => {
  it("never offers to launch a model-written program", () => {
    render(<BinaryCard entry={null} content={content("executable")} />);
    expect(
      screen.queryByRole("button", { name: "Open in system viewer" }),
    ).not.toBeInTheDocument();
    expect(screen.getByText(/does not launch it/)).toBeVisible();
  });

  it("still opens ordinary documents in the system viewer", () => {
    render(<BinaryCard entry={null} content={content("binary")} />);
    expect(
      screen.getByRole("button", { name: "Open in system viewer" }),
    ).toBeVisible();
  });
});
