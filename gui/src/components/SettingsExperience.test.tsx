import { beforeEach, expect, it, vi } from "vitest";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SettingsPage from "./SettingsPage";
import ConnectionStatus from "./settings/ConnectionStatus";
import { makeSettings, catalog } from "../test/settingsFixtures";
import { PREFERENCES_KEY, readAppPreferences } from "../lib/appPreferences";
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  confirm: vi.fn(),
  requestDesktopNotifications: vi.fn(),
  playNotificationSound: vi.fn(),
  deliverNotice: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("./DialogService", () => ({ confirmDialog: mocks.confirm }));
vi.mock("../lib/appNotifications", () => mocks);
beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
  mocks.confirm.mockResolvedValue(false);
  mocks.invoke.mockImplementation(
    async (command: string, args?: { provider?: string }) => {
      if (command === "get_settings")
        return { settings: makeSettings(), warnings: [] };
      if (command === "get_model_catalog")
        return catalog(args?.provider ?? "claude", "api", "test");
      if (command === "list_engines") return [];
      if (command === "get_storage_settings")
        return {
          activeDirectory: "/data",
          configuredDirectory: "/data",
          defaultDirectory: "/data",
          restartRequired: false,
          error: null,
        };
      if (command === "runs_disk_usage") return { count: 2, bytes: 1000 };
      if (command === "workflow_codex_status")
        return {
          account: { status: "signedOut" },
          version: "test",
          epoch: 1,
          loginInProgress: false,
        };
      if (command === "workbench_codex_account_state")
        return { status: "signedOut" };
      if (command === "workbench_get_title_preferences")
        return { enabled: true, model: null, effort: null };
      return undefined;
    },
  );
});
const view = (props = {}) =>
  render(
    <SettingsPage
      onClose={() => {}}
      theme="system"
      onThemeChange={() => {}}
      {...props}
    />,
  );
it("opens General with seven categories and persists appearance, editor, startup, and send preferences", async () => {
  const user = userEvent.setup();
  view();
  expect(await screen.findByRole("heading", { name: "General" })).toBeVisible();
  expect(
    within(
      screen.getByRole("navigation", { name: "Settings categories" }),
    ).getAllByRole("button"),
  ).toHaveLength(7);
  await user.selectOptions(screen.getByLabelText("Interface text size"), "125");
  await user.selectOptions(screen.getByLabelText("Editor font size"), "18");
  await user.click(screen.getByRole("switch", { name: "Wrap long lines" }));
  await user.selectOptions(
    screen.getByLabelText("When Pipeline opens"),
    "home",
  );
  await user.click(screen.getByRole("button", { name: "Conversations" }));
  await user.selectOptions(
    screen.getByLabelText("Send message with"),
    "mod-enter",
  );
  expect(readAppPreferences()).toMatchObject({
    interfaceScale: 125,
    editorFontSize: 18,
    editorWrap: true,
    startup: "home",
    sendShortcut: "mod-enter",
  });
  expect(JSON.parse(localStorage.getItem(PREFERENCES_KEY)!)).toMatchObject({
    startup: "home",
  });
  expect(
    mocks.invoke.mock.calls.some(([name]) => name === "save_settings"),
  ).toBe(false);
});
it("finds synonyms, shows current values, and opens an advanced parser control", async () => {
  const user = userEvent.setup();
  view();
  const search = await screen.findByRole("searchbox");
  await user.type(search, "OCR formula");
  await user.click(
    screen.getByRole("button", { name: /Retain formula numbers/ }),
  );
  const toggle = await screen.findByRole("switch", {
    name: "Retain formula numbers",
  });
  await waitFor(() => expect(toggle).toHaveFocus());
  expect(toggle.closest("details")).toHaveAttribute("open");
});
it("leaves partial keys private during search and asks before discarding a credential draft", async () => {
  const user = userEvent.setup();
  view({ initialSection: "providers" });
  const key = await screen.findByLabelText("Local API Key");
  await user.type(key, "private-key");
  await user.type(screen.getByRole("searchbox"), "startup");
  expect(screen.queryByText(/private-key/)).not.toBeInTheDocument();
  await user.clear(screen.getByRole("searchbox"));
  expect(key).toHaveValue("private-key");
  await user.click(screen.getByRole("button", { name: "General" }));
  expect(mocks.confirm).toHaveBeenCalled();
  expect(key).toBeVisible();
  expect(
    mocks.invoke.mock.calls.some(([name]) => name === "save_settings"),
  ).toBe(false);
  await user.click(screen.getByRole("button", { name: "Save Local API Key" }));
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({ local_api_key: "private-key" }),
    }),
  );
});
it("does not persist an empty retention field and only commits valid input", async () => {
  const user = userEvent.setup();
  view({ initialSection: "storage" });
  const input = await screen.findByLabelText("Review history size limit in GB");
  await user.clear(input);
  await user.tab();
  expect(input).toHaveAttribute("aria-invalid", "true");
  expect(
    mocks.invoke.mock.calls.some(([name]) => name === "save_settings"),
  ).toBe(false);
  await user.type(input, "12.5");
  await user.tab();
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({
        max_saved_run_bytes: 12_500_000_000,
      }),
    }),
  );
  const card = input.closest(".settings-card") as HTMLElement;
  await waitFor(() => expect(within(card).getByText("Saved")).toBeVisible());
});
it("discards an old sign-in check when the connection changes", async () => {
  const user = userEvent.setup();
  let finish!: (value: unknown) => void;
  mocks.invoke.mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const onCheck = vi.fn().mockResolvedValue(undefined);
  const settings = makeSettings();
  const { rerender } = render(
    <ConnectionStatus
      provider="claude"
      settings={settings}
      onCheck={onCheck}
    />,
  );
  await user.click(screen.getByRole("button", { name: "Check connection" }));
  rerender(
    <ConnectionStatus
      provider="claude"
      settings={{ ...settings, claude_access_mode: "api" }}
      onCheck={onCheck}
    />,
  );
  await act(async () =>
    finish({
      readiness: {
        deps: [
          { name: "Claude CLI", found: true, cli_auth_status: "signed_in" },
        ],
      },
    }),
  );
  expect(screen.getByText("API key required")).toBeVisible();
  expect(
    screen.queryByText("Subscription sign-in verified."),
  ).not.toBeInTheDocument();
  expect(onCheck).not.toHaveBeenCalled();
  expect(
    screen.getByRole("button", { name: "Check connection" }),
  ).toBeEnabled();
});
it("keeps the desktop setting off when permission is denied", async () => {
  const user = userEvent.setup();
  mocks.requestDesktopNotifications.mockResolvedValue(false);
  view({ initialSection: "notifications" });
  await user.click(
    await screen.findByRole("switch", { name: "Desktop notifications" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "blocked by your system",
  );
  expect(readAppPreferences().desktopNotifications).toBe(false);
  mocks.requestDesktopNotifications.mockResolvedValue(true);
  await user.click(
    screen.getByRole("switch", { name: "Desktop notifications" }),
  );
  await waitFor(() =>
    expect(readAppPreferences().desktopNotifications).toBe(true),
  );
});
it("reports local preference write failures without applying the change", async () => {
  view();
  const select = await screen.findByLabelText("Interface text size");
  const write = vi.spyOn(localStorage, "setItem").mockImplementation(() => {
    throw new Error("full");
  });
  fireEvent.change(select, { target: { value: "125" } });
  expect(screen.getByRole("alert")).toHaveTextContent(
    "Could not save this preference",
  );
  expect(select).toHaveValue("100");
  write.mockRestore();
});
