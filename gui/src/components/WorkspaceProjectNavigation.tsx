import { useState } from "react";
import { defaultWorkspacePins, loadWorkspacePins, saveWorkspacePins, workspaceDestinations, workspaceSections, type WorkspaceDestination } from "../lib/workspaceNavigation";

export default function WorkspaceProjectNavigation({ workspaceId, destination, disabled, onNavigate, onResetLayout }: {
  workspaceId: string; destination: WorkspaceDestination; disabled: boolean;
  onNavigate: (destination: WorkspaceDestination) => void; onResetLayout: () => void;
}) {
  const [pins, setPins] = useState(() => loadWorkspacePins(workspaceId));
  const [customizing, setCustomizing] = useState(false);
  const update = (next: WorkspaceDestination[]) => { setPins(next); saveWorkspacePins(workspaceId, next); };
  const move = (index: number, delta: number) => {
    const next = [...pins];
    [next[index], next[index + delta]] = [next[index + delta], next[index]];
    update(next);
  };
  return <div className="workspace-project-navigation">
    <nav aria-label="Project sections">
      {workspaceSections.map(section => <button key={section.id} type="button" disabled={disabled}
        aria-current={workspaceDestinations[destination].section === section.id ? "page" : undefined}
        onClick={() => onNavigate(section.destination)}>{section.label}</button>)}
    </nav>
    <div className="workspace-navigation-label"><span>Pinned</span><button type="button" aria-expanded={customizing} onClick={() => setCustomizing(value => !value)}>Customize</button></div>
    <nav aria-label="Pinned project tools">
      {pins.map(id => <button type="button" key={id} disabled={disabled} aria-current={destination === id ? "page" : undefined} onClick={() => onNavigate(id)}>{workspaceDestinations[id].label}</button>)}
      {!pins.length && <p className="px-2 text-xs text-gray-500">Pin tools using Customize.</p>}
    </nav>
    {customizing && <section aria-label="Customize Workspace" className="workspace-customize">
      <p>Choose up to four shortcuts. This changes navigation only.</p>
      {pins.map((id, index) => <div key={id} className="workspace-pin-row">
        <span>{workspaceDestinations[id].label}</span>
        <button type="button" disabled={!index} aria-label={`Move ${workspaceDestinations[id].label} up`} onClick={() => move(index, -1)}>↑</button>
        <button type="button" disabled={index === pins.length - 1} aria-label={`Move ${workspaceDestinations[id].label} down`} onClick={() => move(index, 1)}>↓</button>
        <button type="button" aria-label={`Unpin ${workspaceDestinations[id].label}`} onClick={() => update(pins.filter(value => value !== id))}>×</button>
      </div>)}
      <select aria-label="Pin a project tool" value="" disabled={pins.length >= 4} onChange={event => { if (event.target.value) update([...pins, event.target.value as WorkspaceDestination]); }}>
        <option value="">Pin a tool…</option>
        {(Object.keys(workspaceDestinations) as WorkspaceDestination[]).filter(id => !pins.includes(id)).map(id => <option key={id} value={id}>{workspaceDestinations[id].label}</option>)}
      </select>
      <button type="button" onClick={() => { update([...defaultWorkspacePins]); onResetLayout(); }}>Reset layout and shortcuts</button>
    </section>}
  </div>;
}
