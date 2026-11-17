import useContainerWidth from "../../hooks/useContainerWidth";
import usePersistentPanelWidth from "../../hooks/usePersistentPanelWidth";
import { isAutoReview } from "../../lib/autoReview";
import type { PipelineConfig, StepConfig } from "../../lib/workflowTypes";
import Select from "../../ui/Select";
import SidebarPanel from "../SidebarPanel";
import type { ReactNode } from "react";

interface Entry {
  id: string;
  label: string;
  detail?: string;
  accessibleName?: string;
}
interface Group {
  title: string;
  entries: Entry[];
}

function schemaGroups(config: PipelineConfig): Group[] {
  const stepEntry = (step: StepConfig): Entry => ({
    id: step.id,
    label: step.label,
    detail: [
      step.phase === "parallel" ? "Parallel" : "Sequential",
      !step.enabled && "Disabled",
      step.kind === "analyzer"
        ? "Pipeline writes this result"
        : step.output_schema
          ? "Custom JSON"
          : "Markdown",
    ]
      .filter(Boolean)
      .join(" · "),
    accessibleName: step.label + " output schema",
  });
  return [
    {
      title: "Output formats",
      entries: [
        {
          id: "orientation",
          label: "Document overview",
          accessibleName: "Document overview schema",
          detail: config.orientation_schema
            ? isAutoReview(config)
              ? "Catalog-backed"
              : "Review contract"
            : "Object only",
        },
      ],
    },
    { title: "Step outputs", entries: config.steps.map(stepEntry) },
  ];
}

/** Browse output formats across stages without leaving the schema editor. */
export function SchemaBrowser({
  config,
  selected,
  onSelect,
  children,
}: {
  config: PipelineConfig;
  selected: string | null;
  onSelect: (id: string) => void;
  children: ReactNode;
}) {
  const [root, width] = useContainerWidth<HTMLDivElement>();
  const [panelWidth, setPanelWidth] = usePersistentPanelWidth(
    "pipeline.ui.schemaBrowserWidth",
    250,
    200,
    640,
  );
  const groups = schemaGroups(config);
  const compact = width !== null && width < 680;
  const maxWidth = width ? Math.min(640, Math.floor(width * 0.45)) : 640;
  return (
    <div
      ref={root}
      className="designer-body"
      data-compact={compact || undefined}
    >
      {compact ? (
        <div className="designer-picker">
          <Select
            label="Output format"
            value={selected ?? ""}
            placeholder="Choose a stage"
            options={groups.flatMap((group) =>
              group.entries.map((entry) => ({
                value: entry.id,
                label: entry.label,
                description: [group.title, entry.detail]
                  .filter(Boolean)
                  .join(" · "),
              })),
            )}
            onChange={onSelect}
          />
        </div>
      ) : (
        <SidebarPanel
          aria-label="Schema navigation"
          className="designer-nav"
          width={Math.min(panelWidth, maxWidth)}
          defaultWidth={250}
          min={200}
          max={maxWidth}
          onResize={setPanelWidth}
          resizeLabel="Resize schema panel"
        >
          <div className="designer-outline-title">
            <h2>Schemas</h2>
          </div>
          <nav
            aria-label="Schema stages"
            className="min-h-0 overflow-y-auto pb-4"
          >
            {groups.map((group) => (
              <div key={group.title} className="designer-section">
                {group.title && <h3>{group.title}</h3>}
                {group.entries.map((entry) => (
                  <button
                    type="button"
                    key={entry.id}
                    className="designer-row"
                    aria-label={entry.accessibleName}
                    aria-description={entry.detail || undefined}
                    aria-pressed={entry.id === selected}
                    onClick={() => onSelect(entry.id)}
                  >
                    {entry.label}
                    {entry.detail && <span>{entry.detail}</span>}
                  </button>
                ))}
              </div>
            ))}
          </nav>
        </SidebarPanel>
      )}
      <div className="designer-canvas">{children}</div>
    </div>
  );
}
