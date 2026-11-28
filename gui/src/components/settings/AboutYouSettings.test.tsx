import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import AboutYouSettings from "./AboutYouSettings";
import type {
  ResearcherProfile,
  WritingSample,
} from "../../lib/researcherClient";

const { invoke, open, confirmDialog } = vi.hoisted(() => ({
  invoke: vi.fn(),
  open: vi.fn(),
  confirmDialog: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open }));
vi.mock("../DialogService", () => ({ confirmDialog }));

const sample = (changes: Partial<WritingSample> = {}): WritingSample => ({
  id: "0123456789abcdef",
  title: "Minimum wages and firm entry",
  year: 2024,
  kind: "paper",
  fileName: "minimum_wages.pdf",
  extension: "pdf",
  contentHash: "0123456789abcdef".repeat(4),
  words: 11200,
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
  folder: "/research/Pipeline/researcher",
  maxNoteBytes: 4096,
  maxSamples: 20,
  ...changes,
});

/** Answers `researcher_profile` with `initial` and other commands from `answers`. */
function serve(
  initial: ResearcherProfile,
  answers: Record<string, (args: never) => unknown> = {},
) {
  invoke.mockImplementation((command: string, args: never) => {
    if (command === "researcher_profile") return Promise.resolve(initial);
    const answer = answers[command];
    if (!answer) return Promise.reject(new Error(`Unexpected ${command}`));
    try {
      return Promise.resolve(answer(args));
    } catch (error) {
      return Promise.reject(error);
    }
  });
}

const background = () => screen.findByRole("textbox", { name: "Background" });
const saveButton = () =>
  screen.getByRole("button", { name: "Save background" });

describe("AboutYouSettings", () => {
  beforeEach(() => {
    invoke.mockReset();
    open.mockReset();
    confirmDialog.mockReset();
  });

  it("saves the background and says where it is sent", async () => {
    serve(profile({ about: "Economist." }), {
      researcher_save_note: ({ text }: { text: string }) =>
        profile({ about: text }),
    });
    render(<AboutYouSettings />);
    const field = await background();
    expect(field).toHaveValue("Economist.");
    expect(field).toHaveAccessibleDescription(/Sent with every conversation/);
    expect(saveButton()).toBeDisabled();
    fireEvent.change(field, {
      target: { value: "  Labor economist. British spelling.\n" },
    });
    fireEvent.click(saveButton());
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("researcher_save_note", {
        note: "about",
        text: "Labor economist. British spelling.",
      }),
    );
    expect(await screen.findByText("Saved")).toBeVisible();
    expect(field).toHaveValue("Labor economist. British spelling.");
    expect(saveButton()).toBeDisabled();
  });

  it("cancels an edit and keeps a failed save for another try", async () => {
    serve(profile({ about: "Economist." }), {
      researcher_save_note: () => {
        throw new Error("The folder is read-only");
      },
    });
    render(<AboutYouSettings />);
    const field = await background();
    fireEvent.change(field, { target: { value: "Sociologist." } });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(field).toHaveValue("Economist.");
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();

    fireEvent.change(field, { target: { value: "Sociologist." } });
    fireEvent.click(saveButton());
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not save Background: The folder is read-only",
    );
    expect(field).toHaveValue("Sociologist.");
    expect(saveButton()).toBeEnabled();
  });

  it("keeps the background within the byte limit", async () => {
    serve(profile());
    render(<AboutYouSettings />);
    const field = await background();
    // Two bytes each: within the character count, over the byte limit.
    fireEvent.change(field, { target: { value: "é".repeat(2049) } });
    expect(screen.getByRole("alert")).toHaveTextContent("4 KiB");
    expect(field).toHaveAttribute("aria-invalid", "true");
    expect(saveButton()).toBeDisabled();
    fireEvent.change(field, { target: { value: "é".repeat(2048) } });
    expect(screen.queryByRole("alert")).toBeNull();
    expect(saveButton()).toBeEnabled();
  });

  it("adds chosen papers one at a time and reports the ones it could not add", async () => {
    const added = sample();
    serve(profile(), {
      researcher_add_sample: ({ path }: { path: string }) => {
        if (path.endsWith("data.csv"))
          throw new Error("Add a PDF, Word, TeX, Markdown, or text file.");
        return profile({ samples: [added] });
      },
    });
    open.mockResolvedValue(["C:\\papers\\minimum_wages.pdf", "/tmp/data.csv"]);
    render(<AboutYouSettings />);
    fireEvent.click(await screen.findByRole("button", { name: "Add paper…" }));
    expect(
      await screen.findByDisplayValue("Minimum wages and firm entry"),
    ).toBeVisible();
    expect(open).toHaveBeenCalledWith(
      expect.objectContaining({ multiple: true }),
    );
    expect(screen.getByText("11,200 words · minimum_wages.pdf")).toBeVisible();
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "data.csv: Add a PDF, Word, TeX, Markdown, or text file.",
    );
  });

  it("does nothing when the file picker is cancelled", async () => {
    serve(profile());
    open.mockResolvedValue(null);
    render(<AboutYouSettings />);
    fireEvent.click(await screen.findByRole("button", { name: "Add paper…" }));
    await waitFor(() => expect(open).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("saves a paper's details as they are changed", async () => {
    const original = sample({ year: null });
    const update = vi.fn(({ update }: { update: Partial<WritingSample> }) =>
      profile({ samples: [{ ...original, ...update }] }),
    );
    serve(profile({ samples: [original] }), {
      researcher_update_sample: update,
    });
    render(<AboutYouSettings />);
    const title = await screen.findByRole("textbox", {
      name: "Title of Minimum wages and firm entry",
    });
    fireEvent.change(title, { target: { value: " Minimum wages " } });
    fireEvent.blur(title);
    await waitFor(() =>
      expect(update).toHaveBeenLastCalledWith({
        update: {
          id: original.id,
          title: "Minimum wages",
          year: null,
          kind: "paper",
          useAsExample: true,
        },
      }),
    );
    fireEvent.change(
      await screen.findByRole("combobox", { name: "Kind of Minimum wages" }),
      { target: { value: "grant" } },
    );
    await waitFor(() =>
      expect(update).toHaveBeenLastCalledWith({
        update: expect.objectContaining({
          title: "Minimum wages",
          kind: "grant",
        }),
      }),
    );
    const year = screen.getByRole("textbox", { name: "Year of Minimum wages" });
    // A year that is not one is put back, not saved.
    fireEvent.change(year, { target: { value: "24" } });
    fireEvent.blur(year);
    expect(year).toHaveValue("");
    expect(update).toHaveBeenCalledTimes(2);
    fireEvent.change(year, { target: { value: "2024" } });
    fireEvent.blur(year);
    await waitFor(() =>
      expect(update).toHaveBeenLastCalledWith({
        update: expect.objectContaining({ year: 2024, kind: "grant" }),
      }),
    );
  });

  it("removes a paper only after confirmation", async () => {
    const remove = vi.fn(() => profile());
    serve(profile({ samples: [sample({ words: null })] }), {
      researcher_remove_sample: remove,
    });
    render(<AboutYouSettings />);
    const button = await screen.findByRole("button", {
      name: "Remove Minimum wages and firm entry",
    });
    expect(screen.getByText("No text found · minimum_wages.pdf")).toBeVisible();
    confirmDialog.mockResolvedValueOnce(false);
    fireEvent.click(button);
    await waitFor(() => expect(confirmDialog).toHaveBeenCalledTimes(1));
    expect(confirmDialog.mock.calls[0][0]).toContain(
      "Your own file isn’t touched",
    );
    expect(remove).not.toHaveBeenCalled();
    confirmDialog.mockResolvedValueOnce(true);
    fireEvent.click(button);
    await waitFor(() =>
      expect(remove).toHaveBeenCalledWith({ id: "0123456789abcdef" }),
    );
    await waitFor(() =>
      expect(screen.queryByRole("list", { name: "Your writing" })).toBeNull(),
    );
  });

  it("stops adding at the limit and opens the folder", async () => {
    serve(profile({ samples: [sample()], maxSamples: 1 }), {
      researcher_open_folder: () => null,
    });
    render(<AboutYouSettings />);
    expect(
      await screen.findByRole("button", { name: "Add paper…" }),
    ).toBeDisabled();
    expect(screen.getByText(/holds up to 1 items/)).toBeVisible();
    expect(screen.getByText(/\/research\/Pipeline\/researcher/)).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("researcher_open_folder"),
    );
  });

  it("saves style notes separately from the background", async () => {
    serve(profile({ about: "Economist.", style: "" }), {
      researcher_save_note: ({
        note,
        text,
      }: {
        note: "about" | "style";
        text: string;
      }) => profile({ about: "Economist.", [note]: text }),
    });
    render(<AboutYouSettings />);
    const style = await screen.findByRole("textbox", { name: "Style notes" });
    expect(saveButton()).toBeDisabled();
    fireEvent.change(style, { target: { value: " First person plural. " } });
    // Only the edited card can save; the background is untouched.
    expect(saveButton()).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Save style notes" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("researcher_save_note", {
        note: "style",
        text: "First person plural.",
      }),
    );
    await waitFor(() => expect(style).toHaveValue("First person plural."));
    expect(await background()).toHaveValue("Economist.");
  });

  it("marks a paper as a style example, but not one without text", async () => {
    const readable = sample({ useAsExample: false });
    const scan = sample({
      id: "fedcba9876543210",
      title: "Scanned reply",
      words: null,
      useAsExample: false,
    });
    const update = vi.fn(({ update }: { update: Partial<WritingSample> }) =>
      profile({
        samples: [{ ...readable, ...update }, scan],
      }),
    );
    serve(profile({ samples: [readable, scan] }), {
      researcher_update_sample: update,
    });
    render(<AboutYouSettings />);
    const boxes = await screen.findAllByRole("checkbox", {
      name: "Use as style example",
    });
    expect(boxes[0]).not.toBeChecked();
    expect(boxes[1]).toBeDisabled();
    fireEvent.click(boxes[0]);
    await waitFor(() =>
      expect(update).toHaveBeenCalledWith({
        update: expect.objectContaining({
          id: readable.id,
          useAsExample: true,
        }),
      }),
    );
    await waitFor(() =>
      expect(
        screen.getAllByRole("checkbox", { name: "Use as style example" })[0],
      ).toBeChecked(),
    );
  });

  it("drafts style notes only once a paper is marked as an example", async () => {
    serve(profile({ samples: [sample({ useAsExample: false })] }));
    render(<AboutYouSettings />);
    expect(
      await screen.findByRole("button", { name: "Draft from my papers" }),
    ).toBeDisabled();
    expect(
      screen.getByText(/Mark a paper in Your writing as a style example/),
    ).toBeVisible();
  });

  it("puts a used draft in the field without saving it", async () => {
    const draft = {
      text: "First person plural.\nState the estimate first.",
      papers: ["Minimum wages and firm entry"],
    };
    const save = vi.fn(({ note, text }: { note: string; text: string }) =>
      profile({ samples: [sample()], [note]: text }),
    );
    serve(profile({ style: "Old notes.", samples: [sample()] }), {
      workbench_draft_style_notes: () => draft,
      researcher_save_note: save,
    });
    render(<AboutYouSettings />);
    fireEvent.click(
      await screen.findByRole("button", { name: "Draft from my papers" }),
    );
    const panel = await screen.findByRole("region", {
      name: "Drafted style notes",
    });
    expect(panel).toHaveTextContent(
      "Drafted from 1 paper: Minimum wages and firm entry.",
    );
    expect(panel).toHaveTextContent("State the estimate first.");
    // The saved notes stay as they are until the draft is used and saved.
    const style = screen.getByRole("textbox", { name: "Style notes" });
    expect(style).toHaveValue("Old notes.");
    fireEvent.click(screen.getByRole("button", { name: "Use draft" }));
    expect(style).toHaveValue(draft.text);
    expect(
      screen.queryByRole("region", { name: "Drafted style notes" }),
    ).toBeNull();
    expect(save).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Save style notes" }));
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith({ note: "style", text: draft.text }),
    );
  });

  it("discards a draft and reports a failed one", async () => {
    let fail = false;
    serve(profile({ samples: [sample()] }), {
      workbench_draft_style_notes: () => {
        if (fail)
          throw {
            code: "title_failed",
            message: "Sign in to ChatGPT first.",
          };
        return { text: "Short openings.", papers: ["A", "B"] };
      },
    });
    render(<AboutYouSettings />);
    const button = await screen.findByRole("button", {
      name: "Draft from my papers",
    });
    fireEvent.click(button);
    expect(
      await screen.findByText("Drafted from 2 papers: A; B."),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Discard" }));
    expect(
      screen.queryByRole("region", { name: "Drafted style notes" }),
    ).toBeNull();
    expect(screen.getByRole("textbox", { name: "Style notes" })).toHaveValue(
      "",
    );
    fail = true;
    fireEvent.click(button);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not draft style notes: Sign in to ChatGPT first.",
    );
  });

  it("says so when About you cannot be loaded", async () => {
    invoke.mockRejectedValue(new Error("Research data folder is unavailable"));
    render(<AboutYouSettings />);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "About you could not be loaded: Research data folder is unavailable",
    );
  });
});
