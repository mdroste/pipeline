import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import YourWritingStatus, { writingInUse } from "./YourWritingStatus";
import type { EffectiveHarness } from "../../lib/workbench/harnessTypes";
import type {
  ResearcherProfile,
  WritingSample,
} from "../../lib/researcherClient";

const { invoke, navigate } = vi.hoisted(() => ({
  invoke: vi.fn(),
  navigate: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("../../lib/router", () => ({ router: { navigate } }));

const sample = (changes: Partial<WritingSample>): WritingSample => ({
  id: "0123456789abcdef",
  title: "Paper",
  year: 2024,
  kind: "paper",
  fileName: "paper.pdf",
  extension: "pdf",
  contentHash: "0".repeat(64),
  words: 1000,
  useAsExample: true,
  addedAt: "2026-11-26T00:00:00Z",
  ...changes,
});

const profile = (
  changes: Partial<ResearcherProfile> = {},
): ResearcherProfile => ({
  about: "",
  style: "",
  samples: [],
  folder: "/data/researcher",
  maxNoteBytes: 4096,
  maxSamples: 20,
  ...changes,
});

function harness(
  preset: { id: string; builtIn: boolean; modules?: string[] },
  ownWritingAvailable = true,
): EffectiveHarness {
  return {
    fingerprint: "f",
    preset: { name: preset.id, modules: [], ...preset },
    enabledModules: [],
    unavailableModules: [],
    moduleAvailability: [
      {
        id: "own_writing",
        available: ownWritingAvailable,
        reasons: ownWritingAvailable
          ? []
          : ["About you is turned off for this project or conversation."],
      },
    ],
  } as unknown as EffectiveHarness;
}

const WRITING = { id: "writing", builtIn: true };

beforeEach(() => {
  invoke.mockReset();
  navigate.mockReset();
  localStorage.clear();
});
afterEach(cleanup);

describe("YourWritingStatus", () => {
  it("says nothing for profiles that don't use Your writing", () => {
    for (const preset of [
      { id: "plain", builtIn: true },
      { id: "research", builtIn: true },
      { id: "custom", builtIn: false, modules: ["paper_context"] },
    ]) {
      const { container, unmount } = render(
        <YourWritingStatus effective={harness(preset)} />,
      );
      expect(container).toBeEmptyDOMElement();
      unmount();
    }
    expect(invoke).not.toHaveBeenCalled();
  });

  it("says what the Writing profile uses from About you", async () => {
    invoke.mockResolvedValue(
      profile({
        style: "Short openings.",
        samples: [
          sample({}),
          sample({ id: "1" }),
          sample({ id: "2", useAsExample: false }),
          sample({ id: "3", words: null }),
        ],
      }),
    );
    render(<YourWritingStatus effective={harness(WRITING)} />);
    expect(await screen.findByTestId("your-writing-status")).toHaveTextContent(
      "Using your style notes and 2 papers from About you.",
    );
    expect(invoke).toHaveBeenCalledWith("researcher_profile");
    fireEvent.click(screen.getByRole("button", { name: "About you" }));
    expect(navigate).toHaveBeenCalledWith({
      page: "settings",
      section: "about",
    });
  });

  it("covers a custom profile that turns Your writing on", async () => {
    invoke.mockResolvedValue(profile({ samples: [sample({})] }));
    render(
      <YourWritingStatus
        effective={harness({
          id: "mine",
          builtIn: false,
          modules: ["own_writing"],
        })}
      />,
    );
    expect(await screen.findByTestId("your-writing-status")).toHaveTextContent(
      "Using 1 paper from About you.",
    );
  });

  it("suggests adding some until hidden, and remembers that", async () => {
    invoke.mockResolvedValue(profile());
    const { unmount } = render(
      <YourWritingStatus effective={harness(WRITING)} />,
    );
    expect(await screen.findByTestId("your-writing-status")).toHaveTextContent(
      "Add style notes or past papers in About you so drafts sound like you.",
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Hide this suggestion" }),
    );
    expect(screen.queryByTestId("your-writing-status")).toBeNull();
    unmount();
    render(<YourWritingStatus effective={harness(WRITING)} />);
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
    expect(screen.queryByTestId("your-writing-status")).toBeNull();
  });

  it("says why nothing is used when About you is turned off", () => {
    invoke.mockResolvedValue(profile({ style: "Short openings." }));
    render(<YourWritingStatus effective={harness(WRITING, false)} />);
    expect(screen.getByTestId("your-writing-status")).toHaveTextContent(
      "About you is turned off for this project or conversation",
    );
  });

  it("stays quiet when About you can't be read", async () => {
    invoke.mockRejectedValue(new Error("Research data folder is unavailable"));
    const { container } = render(
      <YourWritingStatus effective={harness(WRITING)} />,
    );
    await vi.waitFor(() => expect(invoke).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });
});

describe("writingInUse", () => {
  it("names only what is sent", () => {
    expect(writingInUse(profile())).toBeNull();
    expect(writingInUse(profile({ style: "  " }))).toBeNull();
    expect(writingInUse(profile({ style: "x" }))).toBe("your style notes");
    expect(
      writingInUse(profile({ samples: [sample({}), sample({ id: "1" })] })),
    ).toBe("2 papers");
    expect(
      writingInUse(profile({ samples: [sample({ useAsExample: false })] })),
    ).toBeNull();
  });
});
