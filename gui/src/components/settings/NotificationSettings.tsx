import { useState } from "react";
import { usePreferenceEditor } from "./PreferenceSettings";
import { SectionHeader, Toggle } from "./controls";
import { SaveFeedback } from "./SaveState";
import {
  deliverNotice,
  playNotificationSound,
  requestDesktopNotifications,
} from "../../lib/appNotifications";

export default function NotificationSettings() {
  const { preferences: p, update, feedback } = usePreferenceEditor();
  const [permissionError, setPermissionError] = useState("");
  const [busy, setBusy] = useState(false);
  return (
    <>
      <SectionHeader
        title="Notify me about"
        description="Applies to conversations, reviews, and automations while Pipeline is running. Results and requests remain available in their original views."
      />
      {(
        [
          [
            "notifyCompletion",
            "Work completed",
            "A reply, review, batch, or automation finishes.",
          ],
          [
            "notifyFailure",
            "Failures",
            "A conversation, review, or automation fails.",
          ],
          [
            "notifyAttention",
            "Attention required",
            "An approval, input, or research check needs your attention.",
          ],
          [
            "suppressFocused",
            "Quiet while I’m using Pipeline",
            "Suppress these notifications and sounds while the app window is focused.",
          ],
        ] as const
      ).map(([key, label, description]) => (
        <div key={key} id={key} className="settings-anchor">
          <Toggle
            label={label}
            description={description}
            checked={p[key]}
            onChange={(value) => update(key, value)}
          />
          {feedback.key === key && <SaveFeedback {...feedback} />}
        </div>
      ))}
      <div id="notification-sound" className="settings-anchor">
        <Toggle
          label="Play a sound"
          description="Play a short tone with enabled notifications."
          checked={p.notificationSound}
          onChange={(value) => {
            update("notificationSound", value);
            if (value) void playNotificationSound().catch(() => {});
          }}
        />
        {feedback.key === "notificationSound" && <SaveFeedback {...feedback} />}
      </div>
      <div id="desktop-notifications" className="settings-anchor">
        <Toggle
          label="Desktop notifications"
          description="Also show notifications through your operating system. Enabling this may ask for notification permission."
          checked={p.desktopNotifications}
          onChange={async (value) => {
            if (busy) return;
            setPermissionError("");
            if (!value) {
              update("desktopNotifications", false);
              return;
            }
            setBusy(true);
            try {
              if (await requestDesktopNotifications())
                update("desktopNotifications", true);
              else
                setPermissionError(
                  "Notifications are blocked by your system. Allow Pipeline notifications in system settings, then try again.",
                );
            } catch {
              setPermissionError(
                "Desktop notifications are unavailable. You can still use in-app notifications.",
              );
            } finally {
              setBusy(false);
            }
          }}
        />
        {busy && <p role="status">Waiting for notification permission…</p>}
        <SaveFeedback
          error={
            permissionError ||
            (feedback.key === "desktopNotifications"
              ? feedback.error
              : undefined)
          }
          saved={feedback.key === "desktopNotifications" && feedback.saved}
        />
      </div>
      <button
        type="button"
        className="settings-button mt-4"
        onClick={() =>
          void deliverNotice({
            id: "preview",
            kind: "completion",
            title: "Notification preview",
            body: "Your notification preferences are working.",
          })
        }
      >
        Test completion notification
      </button>
      <p className="settings-row-description mt-2">
        The test respects completion, focus, sound, and desktop preferences.
        Pipeline must remain running; system notification settings also apply.
      </p>
    </>
  );
}
