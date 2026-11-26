import { useId, type ReactNode } from "react";

export default function ReviewInspectorTabs({
  tabs,
  value,
  onChange,
  children,
}: {
  tabs: { id: string; label: string }[];
  value: string;
  onChange: (value: string) => void;
  children: ReactNode;
}) {
  const id = useId();
  return (
    <>
      <div
        className="review-inspector-tabs"
        role="tablist"
        aria-label="Review options"
        onKeyDown={(event) => {
          const index = tabs.findIndex((tab) => tab.id === value);
          const next =
            event.key === "ArrowRight"
              ? (index + 1) % tabs.length
              : event.key === "ArrowLeft"
                ? (index + tabs.length - 1) % tabs.length
                : event.key === "Home"
                  ? 0
                  : event.key === "End"
                    ? tabs.length - 1
                    : -1;
          if (next < 0) return;
          event.preventDefault();
          onChange(tabs[next].id);
          document.getElementById(`${id}-${tabs[next].id}`)?.focus();
        }}
      >
        {tabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            role="tab"
            id={`${id}-${tab.id}`}
            aria-controls={`${id}-panel`}
            aria-selected={value === tab.id}
            tabIndex={value === tab.id ? 0 : -1}
            onClick={() => onChange(tab.id)}
          >
            {tab.label}
          </button>
        ))}
      </div>
      <div
        className="review-inspector-body"
        role="tabpanel"
        id={`${id}-panel`}
        aria-labelledby={`${id}-${value}`}
      >
        {children}
      </div>
    </>
  );
}
