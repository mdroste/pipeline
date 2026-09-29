import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import ChatgptConnection from "./ChatgptConnection";
const events = vi.hoisted(() => ({ changed: null as null | (() => void) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_name, callback) => {
    events.changed = callback;
    return () => {};
  }),
}));
const mocked = vi.mocked(invoke);
let pending: { loginId: string; epoch: number } | null;
let signedIn: boolean;
let choices: Array<{ source: string; label: string; email: string }>;
beforeEach(() => {
  pending = null;
  signedIn = false;
  choices = [];
  mocked.mockReset();
  mocked.mockImplementation(async (command, args) => {
    if (command === "chatgpt_account_status")
      return {
        account: {
          status: signedIn ? "chatgpt" : "signedOut",
          email: signedIn ? "test@example.invalid" : null,
        },
        epoch: 5,
        version: "0.153.4",
        loginInProgress: !!pending,
        pendingLogin: pending,
        existingAccounts: choices,
      };
    if (command === "chatgpt_login_start") {
      pending = { loginId: "login-1", epoch: 5 };
      return pending;
    }
    if (command === "chatgpt_login_cancel") {
      expect(args).toEqual(pending);
      pending = null;
      return true;
    }
    if (command === "chatgpt_logout") {
      signedIn = false;
      return;
    }
    if (command === "chatgpt_select_existing_account") {
      signedIn = true;
      choices = [];
      return;
    }
    throw new Error(`Unexpected command ${command}`);
  });
});
it("uses one login and can cancel it after the panel is remounted", async () => {
  const view = render(<ChatgptConnection />);
  const login = await screen.findByRole("button", {
    name: "Sign in to ChatGPT",
  });
  await waitFor(() => expect(login).toBeEnabled());
  fireEvent.click(login);
  await screen.findByRole("button", { name: "Cancel sign-in" });
  view.unmount();
  render(<ChatgptConnection />);
  fireEvent.click(
    await screen.findByRole("button", { name: "Cancel sign-in" }),
  );
  await waitFor(() =>
    expect(mocked).toHaveBeenCalledWith("chatgpt_login_cancel", {
      loginId: "login-1",
      epoch: 5,
    }),
  );
  expect(
    mocked.mock.calls.some(
      ([command]) =>
        command.startsWith("workflow_") || command.startsWith("workbench_"),
    ),
  ).toBe(false);
});
it("updates all account controls on the shared event and signs out globally", async () => {
  const onAccountChange = vi.fn();
  render(<ChatgptConnection onAccountChange={onAccountChange} />);
  await screen.findByText("Not signed in to ChatGPT.");
  signedIn = true;
  await act(async () => {
    events.changed?.();
  });
  expect(
    await screen.findByText("Signed in as test@example.invalid"),
  ).toBeInTheDocument();
  expect(onAccountChange).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
  await screen.findByText("Not signed in to ChatGPT.");
  expect(mocked).toHaveBeenCalledWith("chatgpt_logout");
});
it("lets the user choose between conflicting saved accounts", async () => {
  choices = [
    { source: "workspace", label: "Conversations", email: "a@example.invalid" },
    { source: "reviews", label: "Reviews", email: "b@example.invalid" },
  ];
  render(<ChatgptConnection />);
  fireEvent.click(
    await screen.findByRole("button", {
      name: "Use Reviews account (b@example.invalid)",
    }),
  );
  await waitFor(() =>
    expect(mocked).toHaveBeenCalledWith("chatgpt_select_existing_account", {
      source: "reviews",
    }),
  );
  await screen.findByText("Signed in as test@example.invalid");
});
it("shows the active-work explanation when account switching is blocked", async () => {
  const original = mocked.getMockImplementation()!;
  mocked.mockImplementation(async (command, args) => {
    if (command === "chatgpt_login_start")
      throw "Wait for active Conversations and Reviews to finish";
    return original(command, args);
  });
  render(<ChatgptConnection />);
  const login = await screen.findByRole("button", {
    name: "Sign in to ChatGPT",
  });
  await waitFor(() => expect(login).toBeEnabled());
  fireEvent.click(login);
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Wait for active Conversations and Reviews to finish",
  );
});
