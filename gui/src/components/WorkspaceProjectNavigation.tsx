import {
  workspaceDestinations,
  workspaceSections,
  type WorkspaceDestination,
} from "../lib/workspaceNavigation";

export default function WorkspaceProjectNavigation({
  destination,
  disabled,
  onNavigate,
}: {
  destination: WorkspaceDestination;
  disabled: boolean;
  onNavigate: (destination: WorkspaceDestination) => void;
}) {
  return (
    <nav aria-label="Project sections" className="nav-slot">
      {workspaceSections.map((section) => (
        <button
          key={section.id}
          type="button"
          className="nav-child"
          disabled={disabled}
          aria-current={
            workspaceDestinations[destination].section === section.id
              ? "page"
              : undefined
          }
          title={section.description}
          onClick={() => onNavigate(section.destination)}
        >
          <span className="nav-row-label">{section.label}</span>
        </button>
      ))}
    </nav>
  );
}
