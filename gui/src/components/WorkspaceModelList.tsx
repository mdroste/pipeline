/** Compact catalog rows; descriptions follow focus instead of repeating in every row. */
import { useId, useRef, useState } from "react";
import { Icon } from "../ui/icons";
import { moveFocusInList } from "../ui/listKeys";
import type { WorkspaceModel } from "../lib/workbenchTypes";

const OPTION = '[role="option"]:not([aria-disabled="true"])';

export default function WorkspaceModelList({
  models,
  model,
  disabled,
  unknownModel,
  provider,
  onChange,
}: {
  models: WorkspaceModel[];
  model: string;
  disabled: boolean;
  unknownModel: boolean;
  provider: string;
  onChange: (value: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [preview, setPreview] = useState<string | null>(null);
  const list = useRef<HTMLDivElement>(null);
  const descriptionId = useId();
  const automatic = `Uses the default model from your ${provider} connection.`;
  const options = [
    { value: "", label: "Automatic", description: automatic, tag: "" },
    ...(unknownModel
      ? [
          {
            value: model,
            label: model,
            description: "No longer offered. Choose another model.",
            tag: "Unavailable",
          },
        ]
      : []),
    ...models.map((item) => ({
      value: item.model,
      label: item.displayName,
      description: item.description,
      tag: item.isDefault ? "Default" : "",
    })),
  ];
  const visible = options.filter((item) =>
    `${item.label} ${item.value}`
      .toLowerCase()
      .includes(query.trim().toLowerCase()),
  );
  const selected = options.find((item) => item.value === model);
  const described = visible.find((item) => item.value === preview) ?? selected;
  const focusable = visible.some((item) => item.value === model)
    ? model
    : visible[0]?.value;

  return (
    <>
      {models.length > 6 && (
        <label className="workspace-model-search">
          <Icon name="search" className="h-4 w-4 shrink-0" />
          <input
            aria-label="Search models"
            placeholder="Search models…"
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setPreview(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "ArrowDown") {
                event.preventDefault();
                list.current?.querySelector<HTMLElement>(OPTION)?.focus();
              }
            }}
          />
        </label>
      )}
      <div
        ref={list}
        role="listbox"
        aria-label="Model"
        className="workspace-model-list"
        onMouseLeave={() => setPreview(null)}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            (document.activeElement as HTMLElement | null)?.click();
          } else moveFocusInList(event, OPTION);
        }}
      >
        {visible.map((item) => (
          <div
            key={item.value}
            role="option"
            tabIndex={!disabled && item.value === focusable ? 0 : -1}
            aria-selected={item.value === model}
            aria-disabled={disabled || undefined}
            aria-describedby={
              described?.value === item.value ? descriptionId : undefined
            }
            data-label={item.label}
            data-autofocus={item.value === focusable ? "" : undefined}
            title={item.label}
            onFocus={() => setPreview(item.value)}
            onMouseEnter={() => setPreview(item.value)}
            onClick={() => {
              if (!disabled) onChange(item.value);
            }}
            className="workspace-model-option"
          >
            <span className="min-w-0 flex-1 truncate font-medium">
              {item.label}
            </span>
            {item.tag && (
              <span className="workspace-model-tag">{item.tag}</span>
            )}
            <span className="w-4 shrink-0">
              {item.value === model && (
                <Icon name="check" className="h-4 w-4" />
              )}
            </span>
          </div>
        ))}
      </div>
      {visible.length === 0 && (
        <p role="status" className="px-3 py-4 text-ui-meta text-ink-muted">
          No matching models.
        </p>
      )}
      {unknownModel && (
        <p className="px-3 pb-2 text-ui-meta text-warning">
          {model} is no longer offered. Choose another model.
        </p>
      )}
      <p id={descriptionId} className="workspace-model-description">
        {described?.description || "Choose a model for your next message."}
      </p>
    </>
  );
}
