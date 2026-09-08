import type { WorkspaceModel } from "../lib/workbenchTypes";

const effortLabel = (value: string) => ({
  none: "None", minimal: "Minimal", low: "Low", medium: "Medium", high: "High",
  xhigh: "Extra high", max: "Maximum", ultra: "Ultra",
})[value] ?? value;

/** The conversation's catalog-backed choices, shared by Send and follow-ups. */
export default function WorkspaceComposerControls({ models, model, effort, disabled, onChange }: {
  models: WorkspaceModel[]; model: string; effort: string; disabled: boolean;
  onChange: (model: string, effort: string) => void;
}) {
  // Automatic may also include connection defaults; do not infer an effective
  // model from the catalog's isDefault flag when the connection has overrides.
  const selected = models.find(item => item.model === model);
  const supported = selected?.supportedReasoningEfforts ?? [];
  const unknownEffort = effort && !supported.some(item => item.reasoningEffort === effort);
  return <div className="workspace-composer-controls" aria-label="Message settings">
    <label><span>Model</span>
      <select aria-label="Model" value={model} disabled={disabled}
        title={disabled ? "Model settings are unavailable while the conversation is updating or working." : "Applies to this conversation's next message."}
        onChange={event => onChange(event.target.value, "")}>
        <option value="">Auto model</option>
        {model && !selected && <option value={model}>{model} · unavailable</option>}
        {models.map(item => <option key={item.id} value={item.model}>{item.displayName}</option>)}
      </select>
    </label>
    <label><span>Thinking</span>
      <select aria-label="Thinking" value={effort} disabled={disabled || !supported.length}
        title={!model ? "Uses the connection's default thinking. Select a model to choose a supported effort." : !supported.length ? "This model does not advertise adjustable thinking." : "Reasoning effort for your next message."}
        onChange={event => onChange(model, event.target.value)}>
        <option value="">Default thinking{selected?.defaultReasoningEffort ? ` · ${effortLabel(selected.defaultReasoningEffort)}` : ""}</option>
        {unknownEffort && <option value={effort}>{effortLabel(effort)} · unavailable</option>}
        {supported.map(item => <option key={item.reasoningEffort} value={item.reasoningEffort}>{effortLabel(item.reasoningEffort)}</option>)}
      </select>
    </label>
  </div>;
}
