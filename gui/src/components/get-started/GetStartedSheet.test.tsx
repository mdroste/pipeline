import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  GET_STARTED_KEY,
  openGetStarted,
  readGetStarted,
} from "../../lib/getStarted";
import { router } from "../../lib/router";
import type { DepStatus, DepsReport } from "../../lib/types";
import { makeSettings } from "../../test/settingsFixtures";
import GetStartedHost from "./GetStartedHost";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const dep = (overrides: Partial<DepStatus>): DepStatus => ({
  name: "Claude CLI",
  found: true,
  version: "1.0.0",
  path: "/usr/local/bin/claude",
  required: false,
  hint: "",
  ...overrides,
});

const dependencies: DepsReport = {
  ready: true,
  deps: [
    dep({ cli_auth_status: "signed_in" }),
    dep({ name: "Local LLM server", found: false }),
    dep({ name: "pdftoppm" }),
    dep({ name: "pdftotext" }),
    dep({ name: "PaddleOCR-VL Full Parser", found: false }),
  ],
};

const toolchains = {
  platform: "macos",
  tools: [
    {
      kind: "python",
      adapter: "python",
      label: "Python 3.12",
      path: "/usr/bin/python3",
      source: "path",
      version: "3.12.4",
      versionText: "Python 3.12.4",
      versionSource: "probe",
      sha256: null,
      sizeBytes: null,
      note: null,
    },
  ],
  project: null,
  boundary:
    "Detection looked only in standard install folders, your PATH and this project's folder.",
};

function respond(command: string) {
  switch (command) {
    case "chatgpt_account_status":
      return Promise.resolve({
        account: {
          status: "none",
          email: null,
          planType: null,
          unsupportedAccountType: null,
          requiresOpenaiAuth: true,
        },
        epoch: 1,
        version: "0.147.0",
        loginInProgress: false,
      });
    case "get_settings":
      return Promise.resolve({
        settings: { ...makeSettings(), anthropic_api_key: "sk-ant-test" },
        warnings: [],
      });
    case "workbench_detect_toolchains":
      return Promise.resolve(toolchains);
    default:
      return Promise.reject(new Error(`unexpected ${command}`));
  }
}

function renderHost(
  overrides: Partial<Parameters<typeof GetStartedHost>[0]> = {},
) {
  const props = {
    dependencies,
    dependenciesLoading: false,
    onCheckDependencies: vi.fn(),
    onNewReview: vi.fn(),
    onNewProject: vi.fn(),
    ...overrides,
  };
  const view = render(<GetStartedHost {...props} />);
  return { props, ...view };
}

const sheet = () =>
  screen.findByRole("dialog", { name: "Get started with Pipeline" });

beforeEach(() => {
  localStorage.clear();
  invoke.mockReset().mockImplementation(respond);
});

describe("Get started sheet", () => {
  it("opens once on a first launch and stays closed after it is dismissed", async () => {
    const user = userEvent.setup();
    const { unmount } = renderHost();
    const dialog = await sheet();
    expect(
      within(dialog).getByRole("heading", { name: "What do you work with?" }),
    ).toBeVisible();
    await waitFor(() =>
      expect(
        within(dialog).getByRole("radio", { name: /Stata and LaTeX/ }),
      ).toHaveFocus(),
    );
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(readGetStarted()?.dismissedAt).toEqual(expect.any(String));
    unmount();

    renderHost();
    await act(async () => {});
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    // Home and Settings reopen it on request.
    act(() => openGetStarted());
    expect(await sheet()).toBeVisible();
  });

  it("does not open by itself over an existing installation", async () => {
    localStorage.setItem("pipeline.ui.route", "#/home");
    renderHost();
    await act(async () => {});
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(localStorage.getItem(GET_STARTED_KEY)).toBeNull();
  });

  it("shows connection states and links to Settings without changing settings", async () => {
    const user = userEvent.setup();
    const navigate = vi.spyOn(router, "navigate").mockResolvedValue(true);
    renderHost();
    const dialog = await sheet();
    await user.click(
      within(dialog).getByRole("button", { name: "2. Connections" }),
    );
    const list = within(dialog).getByRole("list", {
      name: "Model connections",
    });
    await waitFor(() =>
      expect(within(list).getByText("Not signed in")).toBeVisible(),
    );
    expect(within(list).getByText("Installed and signed in")).toBeVisible();
    expect(within(list).getByText("Saved: Anthropic")).toBeVisible();
    expect(within(list).getByText("Not running")).toBeVisible();
    expect(within(list).getAllByText(/Conversations and Reviews/)).toHaveLength(
      2,
    );
    expect(within(list).getAllByText(/Reviews only/)).toHaveLength(2);
    expect(within(dialog).getByRole("status")).toHaveTextContent(
      "You can start Conversations and Reviews now.",
    );

    await user.click(
      within(dialog).getByRole("button", { name: "Open ChatGPT in Settings" }),
    );
    expect(navigate).toHaveBeenCalledWith({
      page: "settings",
      section: "providers",
      targetId: "workspace-provider",
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("save_settings", expect.anything());
    navigate.mockRestore();
  });

  it("checks this computer only when asked and explains what each tool enables", async () => {
    const user = userEvent.setup();
    renderHost();
    const dialog = await sheet();
    await user.click(
      within(dialog).getByRole("radio", { name: /Python and notebooks/ }),
    );
    await user.click(within(dialog).getByRole("button", { name: "Next" }));
    await user.click(within(dialog).getByRole("button", { name: "Next" }));
    expect(
      within(dialog).getByRole("heading", { name: "Tools on this computer" }),
    ).toHaveFocus();
    expect(invoke).not.toHaveBeenCalledWith(
      "workbench_detect_toolchains",
      expect.anything(),
    );
    const pdf = within(dialog).getByRole("list", { name: "PDF tools" });
    expect(within(pdf).getByText(/PDF text tools/)).toBeVisible();
    expect(within(pdf).getByText(/Full Parser/)).toBeVisible();

    await user.click(
      within(dialog).getByRole("button", { name: "Check this computer" }),
    );
    expect(invoke).toHaveBeenCalledWith("workbench_detect_toolchains", {
      workspaceId: null,
    });
    const tools = await within(dialog).findByRole("list", {
      name: "Tools you work with",
    });
    expect(within(tools).getByText("Python 3.12")).toBeVisible();
    expect(within(tools).getByText("/usr/bin/python3")).toBeVisible();
    expect(
      within(tools).getByText(/records the version and lockfile hashes/),
    ).toBeVisible();
    // The notebook persona also names Jupyter when it is missing.
    expect(within(tools).getByText("Jupyter")).toBeVisible();
    expect(within(tools).getByText(/Not found/)).toBeVisible();
    expect(within(dialog).getByText(/Detection looked only/)).toBeVisible();
  });

  it("tailors next steps to the chosen kind of work and remembers the choice", async () => {
    const user = userEvent.setup();
    const navigate = vi.spyOn(router, "navigate").mockResolvedValue(true);
    const { props } = renderHost();
    let dialog = await sheet();
    await user.click(
      within(dialog).getByRole("radio", { name: /Word and Zotero/ }),
    );
    expect(readGetStarted()?.persona).toBe("word-zotero");
    await user.click(
      within(dialog).getByRole("button", { name: "4. Next steps" }),
    );
    let steps = within(dialog).getByRole("list", {
      name: "Suggested next steps",
    });
    expect(within(steps).getByText("Template: Reference Check")).toBeVisible();
    expect(
      within(steps).queryByText("Template: Replication Package Readiness"),
    ).not.toBeInTheDocument();
    expect(within(dialog).getByText("Tips for Word and Zotero")).toBeVisible();
    expect(
      within(dialog).getByText(/Pipeline only reads from Zotero/),
    ).toBeVisible();

    await user.click(
      within(dialog).getByRole("button", { name: "1. Your work" }),
    );
    await user.click(
      within(dialog).getByRole("radio", { name: /Stata and LaTeX/ }),
    );
    await user.click(
      within(dialog).getByRole("button", { name: "4. Next steps" }),
    );
    steps = within(dialog).getByRole("list", { name: "Suggested next steps" });
    const templates = within(steps)
      .getAllByText(/^Template:/)
      .map((node) => node.textContent);
    expect(templates).toEqual([
      "Template: Replication Package Readiness",
      "Template: Reference Check",
    ]);
    expect(within(steps).getByText(/LaTeX project folder/)).toBeVisible();

    await user.click(
      within(steps).getByRole("button", {
        name: "View Replication Package Readiness in the gallery",
      }),
    );
    expect(navigate).toHaveBeenCalledWith({ page: "gallery" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    act(() => openGetStarted());
    dialog = await sheet();
    expect(
      within(dialog).getByRole("radio", { name: /Stata and LaTeX/ }),
    ).toBeChecked();
    await user.click(
      within(dialog).getByRole("button", { name: "4. Next steps" }),
    );
    await user.click(
      within(dialog).getByRole("button", { name: "New review" }),
    );
    expect(props.onNewReview).toHaveBeenCalledOnce();
    act(() => openGetStarted());
    dialog = await sheet();
    await user.click(
      within(dialog).getByRole("button", { name: "4. Next steps" }),
    );
    await user.click(
      within(dialog).getByRole("button", { name: "New project" }),
    );
    expect(props.onNewProject).toHaveBeenCalledOnce();
    expect(invoke).not.toHaveBeenCalledWith("save_settings", expect.anything());
    navigate.mockRestore();
  });

  it("opens the example Review and returns to the same step", async () => {
    const user = userEvent.setup();
    renderHost();
    const dialog = await sheet();
    await user.click(
      within(dialog).getByRole("button", { name: "4. Next steps" }),
    );
    await user.click(
      within(dialog).getByRole("button", { name: "Open the example" }),
    );
    const example = await screen.findByRole("dialog", {
      name: "Example Review — sample data, no model was called",
    });
    expect(
      screen.queryByRole("dialog", { name: "Get started with Pipeline" }),
    ).toBeNull();
    await user.click(
      within(example).getByRole("button", { name: "Back to setup" }),
    );
    expect(
      within(await sheet()).getByRole("heading", { name: "Next steps" }),
    ).toBeVisible();
    // Visiting the example is not a dismissal.
    expect(readGetStarted()?.dismissedAt ?? null).toBeNull();
  });
});
