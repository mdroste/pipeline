// Only related, fixed views share a tab strip. Specialist pages keep their own title.
import {
  fixedViews,
  sectionLabel,
  viewLinks,
  workspaceDestinations,
  type WorkspaceDestination,
} from "../../lib/workspaceNavigation";
const labels: Partial<Record<WorkspaceDestination, string>> = {
  files: "Folder",
  documents: "Imported documents",
  data: "Data",
  reviews: "Findings",
  edits: "Proposed edits",
  decisions: "Decisions",
};

export default function ProjectSubnav({
  tab,
  onSelect,
}: {
  tab: WorkspaceDestination;
  onSelect: (tab: WorkspaceDestination) => void;
}) {
  const tabs = fixedViews(tab),
    links = viewLinks[tab] ?? [];
  const specialist =
    !tabs.includes(tab) &&
    ![
      "overview",
      "chats",
      "project-tools",
      "project-settings",
      "writing",
      "sources",
    ].includes(tab);
  if (!tabs.length && !links.length && !specialist) return null;
  return (
    <div className="focused-related">
      {tabs.length > 0 && (
        <nav aria-label={sectionLabel(tab) + " views"} className="focused-tabs">
          {tabs.map((id) => (
            <button
              type="button"
              key={id}
              onClick={() => onSelect(id)}
              aria-current={id === tab ? "page" : undefined}
            >
              {labels[id] ?? workspaceDestinations[id].label}
            </button>
          ))}
        </nav>
      )}
      {specialist && (
        <div className="focused-location">
          <span>{sectionLabel(tab)} / </span>
          {workspaceDestinations[tab].label}
        </div>
      )}
      {links.length > 0 && (
        <nav aria-label="Related tools" className="focused-links">
          {links.map((id) => (
            <button key={id} type="button" onClick={() => onSelect(id)}>
              {workspaceDestinations[id].label}
            </button>
          ))}
        </nav>
      )}
    </div>
  );
}
