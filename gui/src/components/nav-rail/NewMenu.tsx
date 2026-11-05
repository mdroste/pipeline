import { useRef, useState } from "react";
import { Icon } from "../../ui/icons";
import { Menu, MenuItem } from "../../ui/Menu";
import { NavIcon } from "./NavRows";

/** The one place to start work, in the sidebar's top corner. */
export default function NewMenu({
  compact = false,
  projectName,
  onNewConversation,
  onNewReview,
  onNewProject,
  onNewAutomation,
}: {
  compact?: boolean;
  /** The open project, which a new conversation joins. */
  projectName?: string | null;
  onNewConversation: () => void;
  onNewReview: () => void;
  onNewProject: () => void;
  onNewAutomation: () => void;
}) {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLButtonElement>(null);
  const toggle = () => setOpen((value) => !value);
  return (
    <>
      {compact ? (
        <NavIcon
          ref={anchor}
          icon="new"
          label="New"
          primary
          aria-haspopup="menu"
          aria-expanded={open}
          onClick={toggle}
        />
      ) : (
        <button
          ref={anchor}
          type="button"
          className="nav-primary"
          aria-haspopup="menu"
          aria-expanded={open}
          onClick={toggle}
        >
          <Icon name="new" className="h-4 w-4" />
          New
          <Icon name="chevron-down" className="h-3.5 w-3.5" />
        </button>
      )}
      <Menu
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={anchor}
        label="New"
        side={compact ? "right" : "bottom"}
        align="start"
        width={288}
      >
        <MenuItem
          icon={<Icon name="assistant" />}
          description={
            projectName
              ? `In ${projectName}`
              : "Ask a question or work through a draft"
          }
          onSelect={onNewConversation}
        >
          New conversation
        </MenuItem>
        <MenuItem
          icon={<Icon name="review" />}
          description="Assess a paper and save the report"
          onSelect={onNewReview}
        >
          New review
        </MenuItem>
        <MenuItem
          icon={<Icon name="project" />}
          description="Keep a paper's files, notes, and results together"
          onSelect={onNewProject}
        >
          New project
        </MenuItem>
        <MenuItem
          icon={<Icon name="tasks" />}
          description="Run work on a schedule or in sequence"
          onSelect={onNewAutomation}
        >
          New automation
        </MenuItem>
      </Menu>
    </>
  );
}
