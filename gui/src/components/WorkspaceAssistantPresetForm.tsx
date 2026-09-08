import { useState } from "react";
import type { EffectiveHarness, HarnessCatalog } from "../lib/workbenchTypes";

/** Quick edits create an explicit copy, so another conversation's preset is safe. */
export default function WorkspaceAssistantPresetForm({ effective, catalog, disabled, onSave }: {
  effective: EffectiveHarness; catalog: HarnessCatalog; disabled: boolean;
  onSave: (name: string, instructions: string, modules: string[]) => void;
}) {
  const [name, setName] = useState(`${effective.preset.name} — custom`);
  const [instructions, setInstructions] = useState(effective.preset.instructions);
  const [modules, setModules] = useState(effective.preset.modules);
  return <details className="workspace-quick-settings">
    <summary>Instructions and tools</summary>
    <fieldset disabled={disabled}>
      <label>Instructions<textarea aria-label="Assistant instructions" rows={6} value={instructions} onChange={event => setInstructions(event.target.value)} /></label>
      <div className="workspace-module-choices">
        {catalog.modules.map(module => {
          const availability = effective.moduleAvailability?.find(item => item.id === module.id);
          return <label key={module.id}>
            <input type="checkbox" checked={modules.includes(module.id)}
              onChange={event => setModules(value => event.target.checked ? [...value, module.id] : value.filter(id => id !== module.id))} />
            <span>{module.name}<small>{availability && !availability.available ? `Unavailable: ${availability.reasons.join("; ")}` : module.description}</small></span>
          </label>;
        })}
      </div>
      <label>Name for your copy<input aria-label="Custom assistant preset name" value={name} onChange={event => setName(event.target.value)} /></label>
      <p>Your copy will be used by this conversation. Other conversations keep their presets. New settings can start a fresh assistant context; earlier messages stay in the transcript.</p>
      <button type="button" disabled={disabled || !name.trim()} onClick={() => onSave(name.trim(), instructions, modules)}>Save as new preset</button>
    </fieldset>
  </details>;
}
