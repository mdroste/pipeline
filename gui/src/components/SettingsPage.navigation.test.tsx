import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SettingsPageComponent from "./SettingsPage";
import type { ComponentProps } from "react";
// Existing Review behavior tests enter Reviews explicitly after the navigation redesign.
const SettingsPage = (props: ComponentProps<typeof SettingsPageComponent>) => (
  <SettingsPageComponent initialSection="workflow" {...props} />
);
import { makeSettings } from "../test/settingsFixtures";
import type { EngineStatus, Settings } from "../lib/types";

const invoke = vi.hoisted(() => vi.fn());
const openUrl = vi.hoisted(() => vi.fn());
const confirmDialog = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: openUrl }));
vi.mock("./DialogService", () => ({ confirmDialog, notify: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

import { mockSettingsLoad } from "../test/settingsPageFixtures";

const mockLoad = (
  settings: Settings,
  warnings: string[] = [],
  engines: EngineStatus[] = [],
) => mockSettingsLoad(invoke, settings, warnings, engines);

describe("SettingsPage navigation", () => {
  beforeEach(() => {
    localStorage.clear();
    invoke.mockReset();
    confirmDialog.mockReset();
    openUrl.mockReset();
    openUrl.mockResolvedValue(undefined);
  });

  it("consolidates workflow settings while keeping connections on Providers", async () => {
    mockLoad(makeSettings());
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );
    expect(await screen.findByText("Model defaults")).toBeInTheDocument();
    for (const label of [
      "Input assessment",
      "Parallel review",
      "Combine results",
      "Sequential steps",
    ])
      await userEvent.click(screen.getAllByText(label)[0]);
    expect(
      screen.getByRole("button", { name: "Change default model" }),
    ).toBeVisible();
    expect(
      screen.queryByRole("combobox", { name: "Default provider" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByText("Provider configuration"),
    ).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Claude API Key")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Model calls at once")).toBeVisible();
    expect(
      screen.getByText(
        "Choose one or more models for steps that run at the same time.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Choose one model for steps that run one at a time."),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Choose one model to combine results when a parallel step uses more than one provider.",
      ),
    ).toBeVisible();
    expect(
      screen.getByText(
        "Reads the inputs and selects the specialist reviewers.",
      ),
    ).toBeVisible();
    const orientationDefaults = screen.getByRole("group", {
      name: "Input assessment providers",
    });
    const parallelDefaults = screen.getByRole("group", {
      name: "Parallel review providers",
    });
    expect(
      orientationDefaults.compareDocumentPosition(parallelDefaults) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    const navigation = screen.getByRole("navigation", {
      name: "Settings categories",
    });
    expect(within(navigation).getAllByRole("button")).toHaveLength(7);
    expect(
      within(navigation).getByRole("button", { name: "Connections" }),
    ).toBeVisible();
    expect(
      within(navigation).getByRole("button", { name: "Reviews" }),
    ).toHaveAttribute("aria-current", "page");
    expect(
      screen.queryByLabelText("Maximum saved reports"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("radio", { name: /^Selected model/ }),
    ).toBeVisible();
    expect(invoke).toHaveBeenCalledWith("get_settings");
  });

  it("preserves pending edits while moving between providers and workflow defaults", async () => {
    const user = userEvent.setup();
    mockLoad(makeSettings());
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );
    await user.selectOptions(
      await screen.findByLabelText("Step time limit"),
      "1800",
    );
    await user.click(screen.getByRole("button", { name: "Connections" }));
    await user.type(screen.getByLabelText("Local API Key"), "test-token");
    await user.click(
      screen.getByRole("button", { name: "Save Local API Key" }),
    );
    await user.click(screen.getByRole("button", { name: "Reviews" }));
    expect(screen.getByLabelText("Step time limit")).toHaveValue("1800");
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: {
          ...makeSettings(),
          step_timeout_secs: 1800,
          local_api_key: "test-token",
        },
      }),
    );
  });

  it("opens directly to a requested settings section", async () => {
    mockLoad(makeSettings());
    const view = render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
        initialSection="llm"
      />,
    );
    expect(await screen.findByText("Model defaults")).toBeVisible();

    view.rerender(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
        initialSection="extraction"
      />,
    );

    expect(
      await screen.findByRole("heading", { name: "PDF extraction" }),
    ).toBeVisible();
    expect(
      screen.getByRole("radio", { name: /^Selected model\b/ }),
    ).toBeVisible();
  });

  it("focuses and scrolls to a requested settings target", async () => {
    const originalScrollIntoView = Object.getOwnPropertyDescriptor(
      HTMLElement.prototype,
      "scrollIntoView",
    );
    const scrollIntoView = vi.fn();
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
      configurable: true,
      value: scrollIntoView,
    });
    try {
      mockLoad(makeSettings());
      render(
        <SettingsPage
          onClose={() => {}}
          theme="light"
          onThemeChange={() => {}}
          initialSection="extraction"
          targetId="paddleocr-local-engine"
          navigationKey={1}
        />,
      );

      const target = await waitFor(() => {
        const element = document.getElementById("paddleocr-local-engine");
        expect(element).not.toBeNull();
        return element as HTMLElement;
      });
      await waitFor(() => {
        expect(scrollIntoView).toHaveBeenCalledWith({ block: "center" });
        expect(target.contains(document.activeElement)).toBe(true);
      });
    } finally {
      if (originalScrollIntoView) {
        Object.defineProperty(
          HTMLElement.prototype,
          "scrollIntoView",
          originalScrollIntoView,
        );
      } else {
        delete (HTMLElement.prototype as Partial<HTMLElement>).scrollIntoView;
      }
    }
  });

  it("shows essential explanations beside their controls", async () => {
    mockLoad(makeSettings());
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );
    expect(
      await screen.findByText(/How many model calls a review can run/),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", {
        name: "More information about Model calls at once",
      }),
    ).not.toBeInTheDocument();
  });

  it("offers system, light, and dark appearance choices", async () => {
    mockLoad(makeSettings());
    const onThemeChange = vi.fn();
    const user = userEvent.setup();
    render(
      <SettingsPage
        onClose={() => {}}
        theme="system"
        onThemeChange={onThemeChange}
        initialSection="general"
      />,
    );
    expect(await screen.findByRole("radio", { name: "System" })).toBeChecked();
    await user.click(screen.getByRole("radio", { name: "Dark" }));
    expect(onThemeChange).toHaveBeenCalledWith("dark");
  });
});
