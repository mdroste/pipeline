import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import ProjectSettings from "./ProjectSettings";
import type { SurfaceApi } from "./shared";
import { makeProjectHome } from "../../test/projectSurfaceFixtures";

afterEach(() => {
  vi.useRealTimers();
});

function mount() {
  const saveSettings = vi.fn().mockResolvedValue(undefined);
  const data = makeProjectHome();
  data.settings.body.targetDate = "2026-10-28";
  render(
    <ProjectSettings
      {...({
        workspaceId: "workspace",
        data,
        workspace: null,
        act: vi.fn(),
        saveSettings,
        reportError: vi.fn(),
      } as unknown as SurfaceApi)}
      watch={false}
      setWatch={vi.fn()}
      onImportPaper={vi.fn()}
      onAttachFolder={vi.fn()}
    />,
  );
  return { saveSettings, field: screen.getByLabelText("Target date") };
}

it("does not save the digits of a year that is still being typed", () => {
  const { saveSettings, field } = mount();
  vi.useFakeTimers();
  fireEvent.focus(field);
  fireEvent.change(field, { target: { value: "0002-10-28" } });
  act(() => vi.advanceTimersByTime(1_000));
  expect(saveSettings).not.toHaveBeenCalled();
  fireEvent.blur(field);
  expect(saveSettings).not.toHaveBeenCalled();
  expect(field).toHaveValue("2026-10-28");
});

it("saves a complete date after a pause, or at once on blur", () => {
  const { saveSettings, field } = mount();
  vi.useFakeTimers();
  fireEvent.focus(field);
  fireEvent.change(field, { target: { value: "2027-10-28" } });
  act(() => vi.advanceTimersByTime(799));
  expect(saveSettings).not.toHaveBeenCalled();
  act(() => vi.advanceTimersByTime(1));
  expect(saveSettings).toHaveBeenCalledWith({ targetDate: "2027-10-28" });
  fireEvent.change(field, { target: { value: "2027-11-30" } });
  fireEvent.blur(field);
  expect(saveSettings).toHaveBeenLastCalledWith({ targetDate: "2027-11-30" });
  act(() => vi.advanceTimersByTime(1_000));
  expect(saveSettings).toHaveBeenCalledTimes(2);
});

it("clears the date when the field is emptied", () => {
  const { saveSettings, field } = mount();
  fireEvent.focus(field);
  fireEvent.change(field, { target: { value: "" } });
  fireEvent.blur(field);
  expect(saveSettings).toHaveBeenCalledWith({ targetDate: null });
});
