import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import WorkflowGalleryPage from "./WorkflowGalleryPage";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

describe("WorkflowGalleryPage", () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === "list_profiles") return Promise.resolve([]);
      if (command === "create_profile") {
        return Promise.resolve({ id: "revision-response-check", name: args?.name, step_count: 2, builtin: false });
      }
      if (command === "save_pipeline_config") return Promise.resolve();
      if (command === "switch_profile") return Promise.resolve((invoke.mock.calls.find(([name]) => name === "save_pipeline_config")?.[1] as { config: unknown }).config);
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
  });

  it("installs a gallery item as an editable local profile and activates it", async () => {
    const user = userEvent.setup();
    const onInstalled = vi.fn();
    render(<WorkflowGalleryPage onInstalled={onInstalled} />);

    const card = (await screen.findByRole("heading", { name: "Revision Response Check" })).closest("article")!;
    await user.click(card.querySelector("button")!);

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_pipeline_config", {
      profileId: "revision-response-check",
      config: expect.objectContaining({ steps: expect.any(Array) }),
    }));
    expect(invoke).toHaveBeenCalledWith("switch_profile", { id: "revision-response-check" });
    expect(onInstalled).toHaveBeenCalledWith(expect.objectContaining({ steps: expect.any(Array) }));
    expect(await screen.findByText(/Installed and activated/)).toBeVisible();
  });

  it("avoids an existing profile id even when its display name differs", async () => {
    invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === "list_profiles") {
        return Promise.resolve([{
          id: "revision-response-check",
          name: "A differently named profile",
          step_count: 1,
          builtin: false,
        }]);
      }
      if (command === "create_profile") {
        return Promise.resolve({
          id: "revision-response-check-2",
          name: args?.name,
          step_count: 2,
          builtin: false,
        });
      }
      if (command === "save_pipeline_config") return Promise.resolve();
      if (command === "switch_profile") return Promise.resolve({ steps: [] });
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const user = userEvent.setup();
    render(<WorkflowGalleryPage onInstalled={() => {}} />);

    const card = (await screen.findByRole("heading", { name: "Revision Response Check" })).closest("article")!;
    await user.click(card.querySelector("button")!);

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("create_profile", {
      name: "Revision Response Check (2)",
    }));
  });
});
