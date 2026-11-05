import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import ConversationTitleSettings from "./ConversationTitleSettings";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const preferences = { enabled: true, model: null, effort: null };

beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation((command: string) => {
    if (command === "workbench_get_title_preferences")
      return Promise.resolve(preferences);
    if (command === "workbench_save_title_preferences")
      return Promise.reject({ message: "Missing" });
    return Promise.resolve({ status: "signedOut" });
  });
});

it("shows the message of a structured save failure", async () => {
  render(<ConversationTitleSettings />);
  fireEvent.click(
    await screen.findByRole("checkbox", {
      name: "Name new conversations automatically",
    }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Could not save conversation titles: Missing",
  );
});

it("shows the message of a structured load failure", async () => {
  invoke.mockImplementation((command: string) =>
    command === "workbench_get_title_preferences"
      ? Promise.reject({ message: "Missing" })
      : Promise.resolve({ status: "signedOut" }),
  );
  render(<ConversationTitleSettings />);
  expect(await screen.findByRole("alert")).toHaveTextContent(/^Missing/);
});
