import { readArgvText } from "../../lib/argvText";
import { button, input } from "../../ui/classes";

/** Each field is one argument; spaces and Windows paths need no shell quoting. */
export default function ExecutionArguments({
  value,
  onChange,
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  const args = readArgvText(value);
  const set = (index: number, value: string) =>
    onChange(
      JSON.stringify((args ?? []).map((arg, i) => (i === index ? value : arg))),
    );
  return (
    <div className="space-y-2">
      {args?.map((arg, index) => (
        <div className="flex gap-2" key={index}>
          <label className="min-w-0 flex-1 text-xs">
            {index === 0 ? "Program" : `Argument ${index}`}
            <input
              className={input}
              value={arg}
              onChange={(event) => set(index, event.target.value)}
            />
          </label>
          {index > 0 && (
            <button
              type="button"
              className={button}
              aria-label={`Remove argument ${index}`}
              onClick={() =>
                onChange(JSON.stringify(args.filter((_, i) => i !== index)))
              }
            >
              Remove
            </button>
          )}
        </div>
      ))}
      {args && (
        <button
          type="button"
          className={button}
          onClick={() => onChange(JSON.stringify([...args, ""]))}
        >
          Add argument
        </button>
      )}
      <details open={!args || undefined}>
        <summary className="text-xs">Advanced argument JSON</summary>
        <textarea
          value={value}
          onChange={(event) => onChange(event.target.value)}
          rows={3}
          aria-label="Argument vector JSON"
          className={input}
        />
      </details>
    </div>
  );
}
