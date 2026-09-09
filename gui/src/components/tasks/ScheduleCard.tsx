import { useState } from "react";
import {
  taskClient,
  taskTime,
  type Schedule,
  type Trigger,
} from "../../lib/taskClient";
import { Timing } from "./Timing";

export default function ScheduleCard({
  schedule,
  onUpdated,
  onError,
}: {
  schedule: Schedule;
  onUpdated: () => void;
  onError: (error: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [trigger, setTrigger] = useState(schedule.trigger);
  const [busy, setBusy] = useState(false);
  async function update(enabled: boolean, value: Trigger | null = null) {
    setBusy(true);
    try {
      await taskClient.updateSchedule(schedule, enabled, value);
      setEditing(false);
      onUpdated();
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <article className="task-schedule">
      <div className="task-section-heading">
        <div>
          <h3>{schedule.name}</h3>
          <p className="task-muted">
            {schedule.enabled
              ? `Next run ${taskTime(schedule.nextDueAt)}`
              : "Paused"}
          </p>
        </div>
        <div className="task-actions">
          <button
            disabled={busy}
            onClick={() => void update(!schedule.enabled)}
          >
            {schedule.enabled ? "Pause" : "Enable"}
          </button>
          <button onClick={() => setEditing((v) => !v)}>Edit timing</button>
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
