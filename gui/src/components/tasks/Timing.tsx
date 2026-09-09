import { useEffect, useState } from "react";
import { taskClient, taskTime, type Trigger } from "../../lib/taskClient";

const zone = () => Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
const localDate = (date: Date) =>
  new Date(date.getTime() - date.getTimezoneOffset() * 60000)
    .toISOString()
    .slice(0, 16);
export function Timing({
  value,
  onChange,
}: {
  value: Trigger;
  onChange: (trigger: Trigger) => void;
}) {
  const [times, setTimes] = useState<number[]>([]);
  const [error, setError] = useState("");
  useEffect(() => {
    let current = true;
    setError("");
    void taskClient
      .times(value)
      .then((v) => {
        if (current) setTimes(v);
      })
      .catch((e) => {
        if (current) setError(String(e));
      });
    return () => {
      current = false;
    };
  }, [value]);
  return (
    <fieldset className="task-timing">
      <legend>When</legend>
      <div className="task-timing-fields">
        <label className="sr-only" htmlFor="task-timing-kind">
          Start rule
        </label>
        <select
          id="task-timing-kind"
          value={value.kind}
          onChange={(e) => {
            const now = Math.floor(Date.now() / 1000);
            onChange(
              e.target.value === "once"
                ? { kind: "once", at: now + 3600 }
                : e.target.value === "interval"
                  ? { kind: "interval", seconds: 86400, anchor: now + 86400 }
                  : e.target.value === "calendar"
                    ? {
                        kind: "calendar",
                        timezone: zone(),
                        hour: 9,
                        minute: 0,
                        weekdays: [0, 1, 2, 3, 4],
                      }
                    : { kind: "now" },
            );
          }}
        >
          <option value="now">Now</option>
          <option value="once">Later</option>
          <option value="interval">At an interval</option>
          <option value="calendar">On a schedule</option>
        </select>
        {value.kind === "once" && (
          <>
            <input
              aria-label="Start date and time"
              type="datetime-local"
              value={localDate(new Date(value.at * 1000))}
              onChange={(e) => {
                if (e.target.value)
                  onChange({
                    ...value,
                    at: Math.floor(new Date(e.target.value).getTime() / 1000),
                  });
              }}
            />
            <button
              type="button"
              onClick={() =>
                onChange({
                  kind: "once",
                  at: Math.floor(Date.now() / 1000) + 3600,
                })
              }
            >
              In one hour
            </button>
          </>
        )}
        {value.kind === "interval" && (
          <>
            <label>
              Every{" "}
              <input
                aria-label="Interval in minutes"
                type="number"
                min={1}
                max={525600}
                value={value.seconds / 60}
                onChange={(e) =>
                  onChange({
                    ...value,
                    seconds: Math.max(1, Number(e.target.value)) * 60,
                  })
                }
              />{" "}
              minutes
            </label>
            <label>
              First run{" "}
              <input
                aria-label="First run"
                type="datetime-local"
                value={localDate(new Date(value.anchor * 1000))}
                onChange={(e) => {
                  if (e.target.value)
                    onChange({
                      ...value,
                      anchor: Math.floor(
                        new Date(e.target.value).getTime() / 1000,
                      ),
                    });
                }}
              />
            </label>
          </>
        )}
        {value.kind === "calendar" && (
          <>
            <input
              aria-label="Time of day"
              type="time"
              value={`${String(value.hour).padStart(2, "0")}:${String(value.minute).padStart(2, "0")}`}
              onChange={(e) => {
                const [hour, minute] = e.target.value.split(":").map(Number);
                if (Number.isFinite(hour + minute))
                  onChange({ ...value, hour, minute });
              }}
            />
            <input
              aria-label="Time zone"
              value={value.timezone}
              onChange={(e) => onChange({ ...value, timezone: e.target.value })}
            />
            <div className="task-weekdays" aria-label="Days of the week">
              {["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].map(
                (day, i) => (
                  <button
                    key={day}
                    type="button"
                    aria-pressed={value.weekdays.includes(i)}
                    onClick={() =>
                      onChange({
                        ...value,
                        weekdays: value.weekdays.includes(i)
                          ? value.weekdays.filter((v) => v !== i)
                          : [...value.weekdays, i],
                      })
                    }
                  >
                    {day}
                  </button>
                ),
              )}
            </div>
          </>
        )}
      </div>
      {error ? (
        <p role="alert">{error}</p>
      ) : (
        times.length > 0 && (
          <p className="task-muted">Next: {times.map(taskTime).join(" · ")}</p>
        )
      )}
      {value.kind !== "now" && (
        <p className="task-muted">
          Runs while Pipeline is open and this computer is awake. Missed runs
          are combined into one. Each run finishes before the next one starts.
        </p>
      )}
    </fieldset>
  );
}
