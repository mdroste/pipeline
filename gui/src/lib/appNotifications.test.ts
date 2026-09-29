import { beforeEach, expect, it, vi } from "vitest";
import {
  automationNotice,
  deliverNotice,
  notificationsAllowed,
  requestDesktopNotifications,
  workspaceNotice,
} from "./appNotifications";
import { DEFAULT_PREFERENCES, saveAppPreferences } from "./appPreferences";
const mocks = vi.hoisted(() => ({
  notify: vi.fn(),
  sendNotification: vi.fn(),
  isPermissionGranted: vi.fn(),
  requestPermission: vi.fn(),
}));
vi.mock("../components/DialogService", () => ({ notify: mocks.notify }));
vi.mock("@tauri-apps/plugin-notification", () => mocks);
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  mocks.isPermissionGranted.mockResolvedValue(false);
  mocks.requestPermission.mockResolvedValue("denied");
});
it("classifies actual task and mission notices, ignores cancellation, and distinguishes conversation outcomes", () => {
  expect(automationNotice("tasks", { id: "a", state: "finished" })?.kind).toBe(
    "completion",
  );
  expect(automationNotice("missions", { id: "a", state: "failed" })?.kind).toBe(
    "failure",
  );
  expect(automationNotice("tasks", { id: "a", state: "waiting" })?.kind).toBe(
    "attention",
  );
  expect(automationNotice("tasks", { id: "a", state: "cancelled" })).toBeNull();
  expect(
    workspaceNotice({
      kind: "turnCompleted",
      epoch: 1,
      turnId: "a",
      status: "completed",
    })?.kind,
  ).toBe("completion");
  expect(
    workspaceNotice({ kind: "turnCompleted", epoch: 1, status: "failed" })
      ?.kind,
  ).toBe("failure");
  expect(
    workspaceNotice({ kind: "turnCompleted", epoch: 1, status: "interrupted" }),
  ).toBeNull();
  expect(
    workspaceNotice({ kind: "serverRequest", epoch: 1, requestId: 42 })?.kind,
  ).toBe("attention");
});
it("respects category and focus preferences independently", () => {
  const p = {
    ...DEFAULT_PREFERENCES,
    notifyCompletion: false,
    suppressFocused: true,
  };
  expect(notificationsAllowed("completion", p, false)).toBe(false);
  expect(notificationsAllowed("failure", p, false)).toBe(true);
  expect(notificationsAllowed("attention", p, true)).toBe(false);
});
it("delivers in-app notices without asking for desktop permission", async () => {
  await deliverNotice(
    { id: "1", kind: "completion", title: "Done", body: "Ready" },
    false,
  );
  expect(mocks.notify).toHaveBeenCalledWith("Done. Ready", "success");
  expect(mocks.requestPermission).not.toHaveBeenCalled();
  expect(mocks.sendNotification).not.toHaveBeenCalled();
});
it("only sends native notifications with existing permission and never includes source content", async () => {
  saveAppPreferences({ desktopNotifications: true });
  const notice = workspaceNotice({
    kind: "turnCompleted",
    epoch: 1,
    turnId: "a",
    status: "completed",
    delta: "private paper content",
  })!;
  await deliverNotice(notice, false);
  expect(mocks.sendNotification).not.toHaveBeenCalled();
  mocks.isPermissionGranted.mockResolvedValue(true);
  await deliverNotice(notice, false);
  expect(mocks.sendNotification).toHaveBeenCalledWith({
    title: "Reply ready",
    body: "Your conversation has a new reply.",
  });
  expect(mocks.requestPermission).not.toHaveBeenCalled();
});
it("requests system permission only from the explicit enable action", async () => {
  expect(await requestDesktopNotifications()).toBe(false);
  expect(mocks.requestPermission).toHaveBeenCalledTimes(1);
  mocks.requestPermission.mockResolvedValue("granted");
  expect(await requestDesktopNotifications()).toBe(true);
});

it("discovery follows attention preferences and provides an action to the affected portfolio", async () => {
  const notice = automationNotice("discovery", {
    id: "portfolio",
    state: "awaitingSelection",
    revision: 4,
  })!;
  saveAppPreferences({ notifyAttention: false });
  await deliverNotice(notice, false);
  expect(mocks.notify).not.toHaveBeenCalled();
  saveAppPreferences({ notifyAttention: true });
  await deliverNotice(notice, false);
  const action = mocks.notify.mock.calls[0][2];
  expect(action.label).toBe("Open portfolio");
  const opened = vi.fn();
  window.addEventListener("pipeline:open-discovery", opened);
  action.run();
  expect(opened.mock.calls[0][0].detail.id).toBe("portfolio");
  window.removeEventListener("pipeline:open-discovery", opened);
});
