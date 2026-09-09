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
    <div className="workspace-project-navigation">
      <nav aria-label="Project sections">
        {workspaceSections.map((section) => (
          <button
            key={section.id}
            type="button"
            disabled={disabled}
            aria-current={
              workspaceDestinations[destination].section === section.id
                ? "page"
                : undefined
            }
            title={section.description}
            onClick={() => onNavigate(section.destination)}
          >
            <span>{section.label}</span>
            <small>{section.description}</small>
          </button>
        ))}
      </nav>
    </div>
  );
}
