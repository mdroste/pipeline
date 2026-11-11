// The tabs under a project section. Every view in a section is a tab, in a
// fixed order, so nothing hides behind a menu.
import {
  sectionTabs,
  workspaceDestinations,
  type WorkspaceDestination,
} from "../../lib/workspaceNavigation";

const tabClass =
  "-mb-px shrink-0 border-b-2 py-2.5 text-ui-label outline-offset-[-2px] transition-colors";
const idle = "border-transparent text-ink-muted hover:text-ink";
const current = "border-[var(--ui-page-accent)] font-medium text-ink";

export default function ProjectSubnav({
  tab,
  stage,
  onSelect,
}: {
  tab: WorkspaceDestination;
  /** The paper's stage, which decides whether Responses is offered yet. */
  stage?: string | null;
  onSelect: (tab: WorkspaceDestination) => void;
}) {
  return (
    <nav
      aria-label={`${workspaceDestinations[tab].section} destinations`}
      className="flex shrink-0 items-center gap-5 overflow-x-auto border-b border-line px-5"
    >
      {sectionTabs(tab, stage).map((id) => (
        <button
          key={id}
          type="button"
          aria-current={id === tab ? "page" : undefined}
          onClick={() => onSelect(id)}
          className={`${tabClass} ${id === tab ? current : idle}`}
        >
          {workspaceDestinations[id].label}
        </button>
      ))}
    </nav>
  );
}
