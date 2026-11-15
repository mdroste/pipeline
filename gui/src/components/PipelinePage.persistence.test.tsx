import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { PipelineConfig } from "../lib/workflowTypes";
import type { ProfileSummary } from "../lib/workflowTypes";
import {
  invoke,
  confirmDialog,
  notify,
  dialogMocks,
  makeConfig,
  mockLoad,
  profiles,
  addBlankStep,
  resetPipelinePageMocks,
} from "./PipelinePage.test-support";
import PipelinePage from "./PipelinePage";

describe("PipelinePage persistence", () => {
  beforeEach(resetPipelinePageMocks);

  it("opens the workflow gallery from the former URL-import position", async () => {
    const user = userEvent.setup();
    const onOpenGallery = vi.fn();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} onOpenGallery={onOpenGallery} />);

    await user.click(
      await screen.findByRole("button", { name: "Review actions" }),
    );
    await user.click(screen.getByRole("menuitem", { name: "Gallery" }));

    expect(onOpenGallery).toHaveBeenCalledOnce();
    expect(
      screen.queryByRole("button", { name: "From URL…" }),
    ).not.toBeInTheDocument();
  });

  it("surfaces import and export dialog plugin failures", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findByRole("combobox", { name: "Active review" });

    dialogMocks.open.mockRejectedValueOnce(new Error("picker unavailable"));
    await user.click(
      await screen.findByRole("button", { name: "Review actions" }),
    );
    await user.click(screen.getByRole("menuitem", { name: "Import" }));
    await waitFor(() => {
      expect(notify).toHaveBeenCalledWith("Import failed: picker unavailable");
    });

    // The backend now owns the save dialog, so an export failure surfaces from
    // the export command itself rather than a JS dialog call.
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_pipeline_config") return Promise.resolve(makeConfig());
      if (cmd === "list_profiles") return Promise.resolve(profiles);
      if (cmd === "get_active_profile") return Promise.resolve("deep-review");
      if (cmd === "save_pipeline_config") return Promise.resolve();
      if (cmd === "export_profile") {
        return Promise.reject(new Error("save picker unavailable"));
      }
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    await user.click(screen.getByRole("button", { name: /^Export/ }));
    await user.click(screen.getByRole("menuitem", { name: /Export review/ }));
    await waitFor(() => {
      expect(notify).toHaveBeenCalledWith(
        "Export failed: save picker unavailable",
      );
    });
  });

  it("serializes profile mutations while a switch is pending", async () => {
    const user = userEvent.setup();
    let finishSwitch!: (config: PipelineConfig) => void;
    const switchResult = new Promise<PipelineConfig>((resolve) => {
      finishSwitch = resolve;
    });
    const availableProfiles: ProfileSummary[] = [
      ...profiles,
      { id: "quick", name: "Quick Review", step_count: 1, builtin: true },
    ];
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_pipeline_config") return Promise.resolve(makeConfig());
      if (cmd === "list_profiles") return Promise.resolve(availableProfiles);
      if (cmd === "get_active_profile") return Promise.resolve("deep-review");
      if (cmd === "switch_profile") return switchResult;
      if (cmd === "get_settings")
        return Promise.reject(new Error("not needed"));
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    render(<PipelinePage onClose={() => {}} />);

    const selector = await screen.findByRole("combobox", {
      name: "Active review",
    });
    await user.selectOptions(selector, "quick");

    expect(selector).toBeDisabled();
    expect(selector).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("button", { name: "Reset" })).toBeDisabled();
    expect(
      invoke.mock.calls.filter((call) => call[0] === "switch_profile"),
    ).toHaveLength(1);

    const quickConfig = makeConfig();
    quickConfig.steps[0].label = "Quick Technical";
    await act(async () => finishSwitch(quickConfig));

    await waitFor(() => {
      expect(selector).toBeEnabled();
      expect(selector).toHaveValue("quick");
      expect(screen.getAllByText("Quick Technical").length).toBeGreaterThan(0);
    });
  });

  it("invalidates a pending reset when the editor unmounts", async () => {
    const user = userEvent.setup();
    confirmDialog.mockResolvedValueOnce(true);
    let finishReset!: (config: PipelineConfig) => void;
    const resetResult = new Promise<PipelineConfig>((resolve) => {
      finishReset = resolve;
    });
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_pipeline_config") return Promise.resolve(makeConfig());
      if (cmd === "list_profiles") return Promise.resolve(profiles);
      if (cmd === "get_active_profile") return Promise.resolve("deep-review");
      if (cmd === "reset_pipeline_config") return resetResult;
      if (cmd === "get_settings")
        return Promise.reject(new Error("not needed"));
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    const onProfileChange = vi.fn();
    const { unmount } = render(
      <PipelinePage onClose={() => {}} onProfileChange={onProfileChange} />,
    );
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("button", { name: "Reset" }));
    unmount();
    await act(async () => finishReset(makeConfig()));

    expect(onProfileChange).not.toHaveBeenCalled();
  });

  it("keeps profile controls locked through deletion and fallback activation", async () => {
    const user = userEvent.setup();
    confirmDialog.mockResolvedValueOnce(true);
    let finishDelete!: () => void;
    const deleteResult = new Promise<void>((resolve) => {
      finishDelete = resolve;
    });
    const remaining: ProfileSummary[] = [
      {
        id: "auto-paper-review",
        name: "Paper Review",
        step_count: 5,
        builtin: true,
      },
    ];
    const customProfiles: ProfileSummary[] = [
      ...remaining,
      { id: "custom", name: "Custom Review", step_count: 2, builtin: false },
    ];
    let profileLists = 0;
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_pipeline_config") return Promise.resolve(makeConfig());
      if (cmd === "list_profiles") {
        profileLists += 1;
        return Promise.resolve(profileLists === 1 ? customProfiles : remaining);
      }
      if (cmd === "get_active_profile") return Promise.resolve("custom");
      if (cmd === "delete_profile") return deleteResult;
      if (cmd === "switch_profile") return Promise.resolve(makeConfig());
      if (cmd === "get_settings")
        return Promise.reject(new Error("not needed"));
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    render(<PipelinePage onClose={() => {}} />);

    const selector = await screen.findByRole("combobox", {
      name: "Active review",
    });
    expect(selector).toHaveValue("custom");
    await user.click(
      await screen.findByRole("button", { name: "Review actions" }),
    );
    await user.click(screen.getByRole("menuitem", { name: "Delete" }));

    expect(selector).toBeDisabled();
    expect(screen.getByRole("button", { name: "New review" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Review actions" }));
    expect(screen.getByRole("menuitem", { name: "Duplicate" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Rename" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Reset" })).toBeDisabled();
    expect(
      invoke.mock.calls.some(([command]) => command === "switch_profile"),
    ).toBe(false);

    await act(async () => finishDelete());
    await waitFor(() => {
      expect(selector).toBeEnabled();
      expect(selector).toHaveValue("auto-paper-review");
    });
    expect(invoke).toHaveBeenCalledWith("delete_profile", { id: "custom" });
    expect(invoke).toHaveBeenCalledWith("switch_profile", {
      id: "auto-paper-review",
    });
  });

  it("does not mark edits made during a save as persisted", async () => {
    const user = userEvent.setup();
    let finishSave!: () => void;
    const pendingSave = new Promise<void>((resolve) => {
      finishSave = resolve;
    });
    const config = makeConfig();
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_pipeline_config") return Promise.resolve(config);
      if (cmd === "list_profiles") return Promise.resolve(profiles);
      if (cmd === "get_active_profile") return Promise.resolve("deep-review");
      if (cmd === "save_pipeline_config") return pendingSave;
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    const onDirtyChange = vi.fn();
    render(<PipelinePage onClose={() => {}} onDirtyChange={onDirtyChange} />);
    await screen.findAllByText("Technical");

    await addBlankStep(user, "First custom step");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(screen.getByRole("button", { name: "Saving..." })).toBeDisabled();

    await addBlankStep(user, "Second custom step");
    await act(async () => finishSave());

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
      expect(onDirtyChange).toHaveBeenLastCalledWith(true);
    });
    expect(screen.queryByText("Saved.")).not.toBeInTheDocument();
  });

  it("does not let an async prompt reset overwrite edits made while it loads", async () => {
    let resolveTemplate!: (value: string) => void;
    const config = makeConfig();
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_pipeline_config") return Promise.resolve(config);
      if (cmd === "list_profiles") return Promise.resolve(profiles);
      if (cmd === "get_active_profile") return Promise.resolve("deep-review");
      if (cmd === "get_settings")
        return Promise.reject(new Error("not needed"));
      if (cmd === "get_default_prompt") {
        return new Promise<string>((resolve) => {
          resolveTemplate = resolve;
        });
      }
      if (cmd === "save_pipeline_config") return Promise.resolve();
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    const user = userEvent.setup();
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("button", { name: "Review settings" }));
    await user.click(screen.getByRole("button", { name: "Reset to generic" }));
    await user.click(
      screen.getByRole("switch", { name: "Reuse shared input context" }),
    );
    await act(async () => resolveTemplate("new default template"));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const saveCall = invoke.mock.calls.find(
        (call) => call[0] === "save_pipeline_config",
      );
      expect(saveCall?.[1].config).toMatchObject({
        context_cache: { enabled: true },
        parallel_context_template: "new default template",
      });
    });
  });

  it("surfaces failures while loading bundled prompt defaults", async () => {
    const config = makeConfig();
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_pipeline_config") return Promise.resolve(config);
      if (cmd === "list_profiles") return Promise.resolve(profiles);
      if (cmd === "get_active_profile") return Promise.resolve("deep-review");
      if (cmd === "get_orientation_defaults") {
        return Promise.reject(new Error("orientation prompt unavailable"));
      }
      if (cmd === "get_default_prompt") {
        return Promise.reject(new Error("parallel template unavailable"));
      }
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    const user = userEvent.setup();
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("button", { name: "Review settings" }));
    await user.click(screen.getByRole("button", { name: "Reset to generic" }));
    await waitFor(() => {
      expect(notify).toHaveBeenCalledWith(
        "Failed to reset the parallel context template: parallel template unavailable",
      );
    });

    notify.mockClear();
    await user.click(
      screen.getByRole("button", { name: "Document bundle preparation" }),
    );
    await user.click(
      screen.getByRole("button", { name: "Insert generic survey" }),
    );
    await waitFor(() => {
      expect(notify).toHaveBeenCalledWith(
        "Failed to load the default document preparation prompt: orientation prompt unavailable",
      );
    });
  });
});
