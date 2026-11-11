import { useEffect, useId, useMemo, useState } from "react";
import { taskClient } from "../../lib/taskClient";
import {
  intervalUnit,
  localZone as zone,
  scheduledTime,
  schedulePreset,
  zoneLabel,
} from "./schedulePresentation";
import { type Trigger } from "../../lib/taskTypes";
import {
  localDateTime,
  localDateTimeHint,
  readLocalDateTime,
} from "../../lib/localDateTime";
import { NumberField } from "./NumberField";

export function Timing({
  value,
  onChange,
}: {
  value: Trigger;
  onChange: (trigger: Trigger) => void;
}) {
  const [times, setTimes] = useState<number[]>([]);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [unit, setUnit] = useState(
    value.kind === "interval" ? intervalUnit(value.seconds) : 86400,
  );
  const zoneList = useId();
  const zones = useMemo(
    () =>
      [
        ...new Set([
          zone(),
          "UTC",
          ...((
            Intl as typeof Intl & {
              supportedValuesOf?: (key: string) => string[];
            }
          ).supportedValuesOf?.("timeZone") ?? []),
        ]),
      ].map((name) => ({
        name,
        label: `${zoneLabel(name)}${name === zone() ? " · This computer" : ""}`,
      })),
    [],
  );
  useEffect(() => {
    if (value.kind === "interval" && value.seconds % unit !== 0)
      setUnit(intervalUnit(value.seconds));
  }, [value, unit]);
  useEffect(() => {
    let current = true;
    setError("");
    setTimes([]);
    setLoading(true);
    void taskClient
      .times(value)
      .then((v) => {
        if (current) {
          setTimes(v);
          setLoading(false);
        }
      })
      .catch((e) => {
        if (current) {
          setError(String(e));
          setLoading(false);
        }
      });
    return () => {
      current = false;
    };
  }, [value]);
  return (
    <fieldset className="task-timing">
      <legend>When</legend>
      <div className="task-actions" aria-label="Schedule presets">
        {(["hourly", "daily", "weekdays", "weekly"] as const).map((preset) => (
          <button
            type="button"
            key={preset}
            onClick={() => {
              const next = schedulePreset(
                preset,
                value.kind === "calendar" ? value.timezone : zone(),
              );
              if (next.kind === "interval") setUnit(3600);
              onChange(next);
            }}
          >
            {
              {
                hourly: "Hourly",
                daily: "Daily",
                weekdays: "Weekdays",
                weekly: "Weekly",
              }[preset]
            }
          </button>
        ))}
      </div>
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
              title={localDateTimeHint}
              value={localDateTime(new Date(value.at * 1000))}
              onChange={(e) => {
                const at = readLocalDateTime(e.target);
                if (at !== null)
                  onChange({
                    ...value,
                    at,
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
              <NumberField
                aria-label="Interval amount"
                min={1}
                max={31536000 / unit}
                value={value.seconds / unit}
                onChange={(amount) =>
                  onChange({ ...value, seconds: amount * unit })
                }
              />{" "}
            </label>
            <select
              aria-label="Interval unit"
              value={unit}
              onChange={(e) => {
                const next = Number(e.target.value);
                onChange({
                  ...value,
                  seconds: Math.min(
                    31536000,
                    Math.max(next, Math.round(value.seconds / unit) * next),
                  ),
                });
                setUnit(next);
              }}
            >
              <option value={60}>minutes</option>
              <option value={3600}>hours</option>
              <option value={86400}>days</option>
            </select>
            <label>
              First run{" "}
              <input
                aria-label="First run"
                type="datetime-local"
                title={localDateTimeHint}
                value={localDateTime(new Date(value.anchor * 1000))}
                onChange={(e) => {
                  const anchor = readLocalDateTime(e.target);
                  if (anchor !== null)
                    onChange({
                      ...value,
                      anchor,
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
              list={zoneList}
              value={value.timezone}
              onChange={(e) => onChange({ ...value, timezone: e.target.value })}
            />
            <datalist id={zoneList}>
              {zones.map(({ name, label }) => (
                <option key={name} value={name}>
                  {label}
                </option>
              ))}
            </datalist>
            <p className="task-muted">
              {zoneLabel(value.timezone)}
              {value.timezone === zone() ? " · This computer’s time zone" : ""}
            </p>
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
      ) : loading ? (
        <p className="task-muted" role="status">
          Checking upcoming times…
        </p>
      ) : (
        times.length > 0 && (
          <p className="task-muted">
            Next:{" "}
            {times
              .map((at) =>
                scheduledTime(
                  at,
                  value.kind === "calendar" ? value.timezone : zone(),
                ),
              )
              .join(" · ")}{" "}
            · {value.kind === "calendar" ? value.timezone : zone()}
          </p>
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
