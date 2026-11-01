// The tabs under a project section. A section shows its main views, and the
// rest sit under More, so nine equal choices never compete for attention.
import { useRef, useState } from "react";
import {
  sectionTabs,
  workspaceDestinations,
  type WorkspaceDestination,
} from "../../lib/workspaceNavigation";
import { Menu, MenuItem } from "../../ui";
import { Icon } from "../../ui/icons";

const tabClass =
  "-mb-px shrink-0 border-b-2 py-2.5 text-ui-label outline-offset-[-2px] transition-colors";
const idle = "border-transparent text-ink-muted hover:text-ink";
const current = "border-[var(--ui-page-accent)] font-medium text-ink";

export default function ProjectSubnav({
  tab,
  onSelect,
}: {
  tab: WorkspaceDestination;
  onSelect: (tab: WorkspaceDestination) => void;
}) {
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const { tabs, more } = sectionTabs(tab);
  return (
    <nav
      aria-label={`${workspaceDestinations[tab].section} destinations`}
      className="flex shrink-0 items-center gap-5 overflow-x-auto border-b border-line px-5"
    >
      {tabs.map((id) => (
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
      {more.length > 0 && (
        <>
          <button
            ref={button}
            type="button"
            aria-haspopup="menu"
            aria-expanded={open}
            onClick={() => setOpen(!open)}
            className={`${tabClass} ${idle} inline-flex items-center gap-1`}
          >
            More
            <Icon name="chevron-down" className="h-3.5 w-3.5" />
          </button>
          <Menu
            open={open}
            onClose={() => setOpen(false)}
            anchorRef={button}
            label={`More ${workspaceDestinations[tab].section} views`}
            align="start"
          >
            {more.map((id) => (
              <MenuItem key={id} onSelect={() => onSelect(id)}>
                {workspaceDestinations[id].label}
              </MenuItem>
            ))}
          </Menu>
        </>
      )}
    </nav>
  );
}
