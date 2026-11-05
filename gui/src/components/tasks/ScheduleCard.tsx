import { useState } from "react";
import { taskClient } from "../../lib/taskClient";
import { stateLabel, taskTime } from "../../lib/taskPresentation";
import { type Schedule, type Trigger } from "../../lib/taskTypes";
import { confirmDialog } from "../DialogService";
import { Timing } from "./Timing";

export default function ScheduleCard({
  schedule,
  onUpdated,
  onError,
  onOpenRun,
}: {
  schedule: Schedule;
  onUpdated: () => void;
  onError: (error: string) => void;
  onOpenRun?: (id: string) => void | Promise<void>;
}) {
  const [editing, setEditing] = useState(false);
  const [trigger, setTrigger] = useState(schedule.trigger);
  const [busy, setBusy] = useState(false);
  async function update(
    enabled: boolean,
    value: Trigger | null = null,
    refresh = false,
  ) {
    setBusy(true);
    try {
      await taskClient.updateSchedule(schedule, enabled, value, refresh);
      setEditing(false);
      onUpdated();
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  }
  async function remove() {
    const confirmed = await confirmDialog(
      "No further runs will start. Past runs stay in History, and a run in progress keeps going.",
      {
        title: `Delete ${schedule.name}?`,
        confirmLabel: "Delete schedule",
        cancelLabel: "Keep schedule",
        destructive: true,
      },
    );
    if (!confirmed) return;
    setBusy(true);
    try {
      await taskClient.deleteSchedule(schedule);
      onUpdated();
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  }
  const held =
    schedule.enabled &&
    schedule.blockingRun &&
    (schedule.nextDueAt ?? 0) * 1000 <= Date.now()
      ? schedule.blockingRun
      : null;
  return (
    <article className="task-schedule">
      <div className="task-section-heading">
        <div>
          <h3>{schedule.name}</h3>
          <p className="task-muted">
            {held
              ? `Waiting for the previous run · ${stateLabel(held.state)}`
              : schedule.enabled
                ? `Next run ${taskTime(schedule.nextDueAt)}`
                : "Paused"}
          </p>
        </div>
        <div className="task-actions">
          {held && onOpenRun && (
            <button
              onClick={() => {
                void Promise.resolve(onOpenRun(held.id)).catch((e) =>
                  onError(String(e)),
                );
              }}
            >
              Open previous run
            </button>
          )}
          <button
            disabled={busy}
            onClick={() => void update(!schedule.enabled)}
          >
            {schedule.enabled ? "Pause" : "Enable"}
          </button>
          <button onClick={() => setEditing((v) => !v)}>Edit timing</button>
          {schedule.scope.sessionId && (
            <button
              disabled={busy}
              title="Later runs use the conversation's current permissions, folder, and reviews. A changed connection needs a new schedule."
              onClick={() => void update(schedule.enabled, null, true)}
            >
              Use current conversation settings
            </button>
          )}
          <button disabled={busy} onClick={() => void remove()}>
            Delete schedule
          </button>
        </div>
      </div>
      {editing && (
        <>
          <Timing value={trigger} onChange={setTrigger} />
          <button
            className="task-primary"
            disabled={busy || trigger.kind === "now" || trigger.kind === "once"}
            onClick={() => void update(schedule.enabled, trigger)}
          >
            Save schedule
          </button>
        </>
      )}
    </article>
  );
}
