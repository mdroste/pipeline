import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import OverviewOptions from "./OverviewOptions";
import { makeProjectHome } from "../../test/projectSurfaceFixtures";
import type { SurfaceApi } from "./shared";

vi.mock("../../lib/repositoryClient", () => ({
  repositoryClient: { status: vi.fn().mockResolvedValue(null) },
}));

it("edits real project fields, retains exclusions between sections, and routes specialist tools", () => {
  const saveSettings = vi.fn().mockResolvedValue(undefined),
    setTab = vi.fn();
  render(
    <OverviewOptions
      {...({
        workspaceId: "ws",
        data: makeProjectHome(),
        workspace: { root: "/research" },
        saveSettings,
        setTab,
        act: vi.fn(),
        reportError: vi.fn(),
      } as unknown as SurfaceApi)}
      watch={false}
      setWatch={vi.fn()}
      onImportPaper={vi.fn()}
      onAttachFolder={vi.fn()}
    />,
  );
  fireEvent.change(screen.getByLabelText("Venue"), {
    target: { value: "Econometrica" },
  });
  fireEvent.blur(screen.getByLabelText("Venue"));
  expect(saveSettings).toHaveBeenCalledWith({ venue: "Econometrica" });
  fireEvent.change(screen.getByLabelText("Stage of the paper"), {
    target: { value: "revising" },
  });
  expect(saveSettings).toHaveBeenCalledWith({ stage: "revising" });
  fireEvent.click(screen.getByRole("button", { name: "Exclusions" }));
  fireEvent.change(screen.getByLabelText("Paths to leave out"), {
    target: { value: "drafts\n scratch " },
  });
  fireEvent.click(screen.getByRole("button", { name: "Project" }));
  fireEvent.click(screen.getByRole("button", { name: "Exclusions" }));
  expect(screen.getByLabelText("Paths to leave out")).toHaveValue(
    "drafts\n scratch ",
  );
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(saveSettings).toHaveBeenCalledWith({
    ignoredPaths: ["drafts", "scratch"],
  });
  fireEvent.click(screen.getByRole("button", { name: "More tools" }));
  fireEvent.click(screen.getByRole("button", { name: "Command settings →" }));
  expect(setTab).toHaveBeenCalledWith("execution");
  fireEvent.click(screen.getByRole("button", { name: "Share project →" }));
  expect(setTab).toHaveBeenCalledWith("exchange");
});
