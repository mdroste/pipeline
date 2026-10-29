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
    expect(await screen.findByText("Preferred Provider")).toBeInTheDocument();
    for (const label of [
      "Input assessment",
      "Parallel review",
      "Combine results",
      "Sequential steps",
    ])
      await userEvent.click(screen.getAllByText(label)[0]);
    expect(
      screen.getByRole("option", { name: "Claude (Anthropic)" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("option", { name: "ChatGPT (OpenAI)" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("combobox", { name: "Preferred Provider" }),
    ).toBeVisible();
    expect(
      screen.queryByText("Provider configuration"),
    ).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Claude API Key")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Maximum Concurrent Agents")).toBeVisible();
    expect(
      screen.getByText("or more", { selector: "strong" }).closest("p"),
    ).toHaveTextContent(
      "Select one or more default model to use for parallel workflow steps.",
    );
    expect(
      screen.getByText("one", { selector: "strong" }).closest("p"),
    ).toHaveTextContent(
      "Select one default model to use for sequential workflow steps.",
    );
    expect(
      screen.getByText(
        "Select one default model to combine outputs when a Parallel step runs with multiple providers.",
      ),
    ).toBeVisible();
    expect(
      screen.getByText(
        "Choose the default model for reading inputs and selecting specialist reviewers.",
      ),
    ).toBeVisible();
    const orientationDefaults = screen.getByRole("group", {
      name: "Orientation map providers",
    });
    const parallelDefaults = screen.getByRole("group", {
      name: "Parallel steps providers",
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
    expect(screen.getByRole("radio", { name: /^LLM/ })).toBeVisible();
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
      await screen.findByLabelText("Preferred Provider"),
      "codex",
    );
    await user.click(screen.getByRole("button", { name: "Connections" }));
    await user.type(screen.getByLabelText("Local API Key"), "test-token");
    await user.click(
      screen.getByRole("button", { name: "Save Local API Key" }),
    );
    await user.click(screen.getByRole("button", { name: "Reviews" }));
    expect(screen.getByLabelText("Preferred Provider")).toHaveValue("codex");
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: {
          ...makeSettings(),
          preferred_provider: "codex",
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
    expect(await screen.findByText("Preferred Provider")).toBeVisible();

    view.rerender(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
        initialSection="extraction"
      />,
    );

    expect(
      await screen.findByRole("heading", { name: "PDF Extraction" }),
    ).toBeVisible();
    expect(screen.getByRole("radio", { name: /^LLM\b/ })).toBeVisible();
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
      await screen.findByText(/Used when a workflow has no assigned provider/),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", {
        name: "More information about Preferred Provider",
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
