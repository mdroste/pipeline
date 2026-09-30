import { readAppPreferences, type AppPreferences } from "./appPreferences";
import type { WorkbenchEvent } from "./workbenchTypes";
import { router } from "./router";
import { notify } from "../components/DialogService";

export type NoticeKind = "completion" | "failure" | "attention";
export interface AppNotice {
  id: string;
  kind: NoticeKind;
  title: string;
  body: string;
  discoveryId?: string;
}
export function notificationsAllowed(
  kind: NoticeKind,
  p: AppPreferences,
  focused: boolean,
) {
  return (
    !(p.suppressFocused && focused) &&
    (kind === "completion"
      ? p.notifyCompletion
      : kind === "failure"
        ? p.notifyFailure
        : p.notifyAttention)
  );
}

export function workspaceNotice(event: WorkbenchEvent): AppNotice | null {
  const id = `${event.epoch}:${event.threadId ?? ""}:${event.turnId ?? ""}`;
  if (event.kind === "serverRequest")
    return {
      id: `request:${id}:${event.requestId}`,
      kind: "attention",
      title: "Conversation needs attention",
      body: "Open Conversations to review a request.",
    };
  if (event.kind !== "turnCompleted") return null;
  if (event.status === "completed")
    return {
      id: `turn:${id}`,
      kind: "completion",
      title: "Reply ready",
      body: "Your conversation has a new reply.",
    };
  if (event.status === "failed")
    return {
      id: `turn:${id}`,
      kind: "failure",
      title: "Conversation failed",
      body: "Open the conversation to inspect the error.",
    };
  return null; // Stopped turns and connection lifecycle events are not failures.
}

export function automationNotice(
  source: string,
  value: {
    id: string;
    state: string;
    reason?: string | null;
    revision?: number;
  },
): AppNotice | null {
  const kind: NoticeKind | null = [
    "completed",
    "finished",
    "done",
    "succeeded",
  ].includes(value.state)
    ? "completion"
    : ["failed", "error"].includes(value.state)
      ? "failure"
      : [
            "attention",
            "waiting",
            "running",
            "blocked",
            "exhausted",
            "awaitingSelection",
            "partial",
          ].includes(value.state)
        ? "attention"
        : null;
  if (!kind) return null;
  return {
    id: `${source}:${value.id}:${value.state}:${value.revision ?? value.reason ?? ""}`,
    ...(source === "discovery" ? { discoveryId: value.id } : {}),
    kind,
    title:
      kind === "completion"
        ? "Automation complete"
        : kind === "failure"
          ? "Automation failed"
          : "Automation needs attention",
    body: "Open Automations to inspect the result or next action.",
  };
}

let audioContext: AudioContext | null = null;
export async function playNotificationSound() {
  if (typeof AudioContext === "undefined") return;
  audioContext ??= new AudioContext();
  await audioContext.resume();
  const tone = audioContext.createOscillator();
  const volume = audioContext.createGain();
  const start = audioContext.currentTime;
  tone.frequency.value = 660;
  volume.gain.setValueAtTime(0.045, start);
  volume.gain.exponentialRampToValueAtTime(0.001, start + 0.18);
  tone.connect(volume);
  volume.connect(audioContext.destination);
  tone.start(start);
  tone.stop(start + 0.2);
  tone.onended = () => {
    tone.disconnect();
    volume.disconnect();
  };
}

export async function requestDesktopNotifications() {
  const native = await import("@tauri-apps/plugin-notification");
  return (
    (await native.isPermissionGranted()) ||
    (await native.requestPermission()) === "granted"
  );
}

export async function deliverNotice(
  notice: AppNotice,
  focused = document.hasFocus() && document.visibilityState === "visible",
) {
  const p = readAppPreferences();
  if (!notificationsAllowed(notice.kind, p, focused)) return;
  const message = `${notice.title}. ${notice.body}`;
  const kind =
    notice.kind === "failure"
      ? "error"
      : notice.kind === "completion"
        ? "success"
        : "info";
  if (notice.discoveryId) {
    notify(message, kind, {
      label: "Open portfolio",
      run: () =>
        void router.navigate({
          page: "tasks",
          discoveryId: notice.discoveryId,
        }),
    });
  } else {
    notify(message, kind);
  }
  if (p.notificationSound) void playNotificationSound().catch(() => {});
  if (p.desktopNotifications) {
    try {
      const native = await import("@tauri-apps/plugin-notification");
      // Background events never request permission; enabling the setting owns it.
      if (await native.isPermissionGranted())
        native.sendNotification({ title: notice.title, body: notice.body });
    } catch {
      /* The in-app result remains available when native delivery fails. */
    }
  }
}
