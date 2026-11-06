// Project contents stay in the same order as the researcher moves between views.
import {
  workspaceDestinations,
  workspaceSections,
  type WorkspaceDestination,
} from "../lib/workspaceNavigation";
import { Icon } from "../ui/icons";
import { NavRow } from "./nav-rail/NavRows";

export default function WorkspaceProjectNavigation({
  destination,
  disabled,
  onNavigate,
  hasFolder = true,
}: {
  destination: WorkspaceDestination;
  disabled: boolean;
  onNavigate: (destination: WorkspaceDestination) => void;
  hasFolder?: boolean;
}) {
  const current = workspaceDestinations[destination].section;
  return (
    <nav aria-label="Project sections" className="nav-slot">
      {workspaceSections.map((section) => (
        <NavRow
          key={section.id}
          icon={section.icon}
          label={section.label}
          current={section.id === current}
          disabled={disabled}
          title={section.description}
          onClick={() =>
            onNavigate(
              section.id === "files" && !hasFolder
                ? "documents"
                : section.destination,
            )
          }
        />
      ))}
      <div className="focused-project-utilities">
        {(["project-tools", "project-settings"] as const).map((id) => (
          <button
            key={id}
            type="button"
            className="nav-row"
            aria-current={
              current === workspaceDestinations[id].section ? "page" : undefined
            }
            disabled={disabled}
            onClick={() => onNavigate(id)}
          >
            <Icon name={id === "project-settings" ? "settings" : "grid"} />
            {workspaceDestinations[id].label}
          </button>
        ))}
      </div>
    </nav>
  );
}
