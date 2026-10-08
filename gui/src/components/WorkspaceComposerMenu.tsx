import { lazy, Suspense, useRef, useState } from "react";
import IconButton from "../ui/IconButton";
import Popover from "../ui/Popover";
import Button from "../ui/Button";
import Skeleton from "../ui/Skeleton";
import { Menu, MenuItem } from "../ui/Menu";
import { Icon } from "../ui/icons";
import type { ConversationSnapshot } from "../lib/workbenchTypes";

const WorkspaceAttachmentsPanel = lazy(
  () => import("./WorkspaceAttachmentsPanel"),
);
type View = "menu" | "import" | "documents" | null;
const titles = { import: "Add files", documents: "Project documents" };

/** The composer's attach menu: only things that are added to a message. */
export default function WorkspaceComposerMenu({
  snapshot,
  disabled,
  onSnapshot,
  onBusy,
  onBrowseProjects,
  onTasks,
}: {
  snapshot: ConversationSnapshot | null;
  disabled: boolean;
  onSnapshot: (next: ConversationSnapshot) => void;
  onBusy: (busy: boolean) => void;
  /** Open project navigation, for a conversation that has no project yet. */
  onBrowseProjects: () => void;
  onTasks?: () => void;
}) {
  const [view, setView] = useState<View>(null);
  const [busy, setBusy] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const close = () => {
    setView(null);
    trigger.current?.focus();
  };
  const panel = view === "import" || view === "documents" ? view : null;
  return (
    <>
      <IconButton
        ref={trigger}
        label="Add to message"
        tooltip="Add files, a project document, or an automation"
        aria-haspopup="menu"
        aria-expanded={view !== null}
        disabled={disabled && !busy}
        onClick={() => {
          if (!busy) setView(view ? null : "menu");
        }}
      >
        <Icon name="plus" />
      </IconButton>
      <Menu
        open={view === "menu"}
        onClose={() => setView(null)}
        anchorRef={trigger}
        label="Add to message"
        side="top"
        align="start"
        width={300}
      >
        <MenuItem
          keepOpen
          icon={<Icon name="import" />}
          description="Documents, data, and research code from your computer"
          onSelect={() => setView("import")}
        >
          Add files
        </MenuItem>
        <MenuItem
          keepOpen
          icon={<Icon name="file" />}
          description="Use something already saved in this project"
          onSelect={() => setView("documents")}
        >
          Add a project document
        </MenuItem>
        {onTasks && (
          <MenuItem
            disabled={!snapshot}
            icon={<Icon name="tasks" />}
            description="Review, revise, schedule, or wait for input"
            onSelect={onTasks}
          >
            Run an automation
          </MenuItem>
        )}
      </Menu>
      <Popover
        open={panel !== null}
        onClose={() => setView(null)}
        anchorRef={trigger}
        label={panel ? titles[panel] : "Add to message"}
        side="top"
        align="start"
        width={340}
        dismissible={!busy}
        className="p-3"
      >
        {panel && (
          <>
            <div className="mb-3 flex items-center gap-1">
              <IconButton
                size="sm"
                label="Back"
                tooltip={null}
                disabled={busy}
                onClick={() => setView("menu")}
              >
                <Icon name="arrow" className="h-4 w-4 rotate-180" />
              </IconButton>
              <h2 className="min-w-0 flex-1 truncate text-ui-label font-semibold">
                {titles[panel]}
              </h2>
              <IconButton
                size="sm"
                label="Close"
                tooltip={null}
                disabled={busy}
                onClick={close}
              >
                <Icon name="close" className="h-4 w-4" />
              </IconButton>
            </div>
            {snapshot?.session.workspaceId ? (
              <Suspense
                fallback={<Skeleton label="Loading documents" lines={3} />}
              >
                <WorkspaceAttachmentsPanel
                  key={panel}
                  mode={panel}
                  snapshot={snapshot}
                  onSnapshot={onSnapshot}
                  onBusy={(value) => {
                    setBusy(value);
                    onBusy(value);
                  }}
                  onClose={close}
                />
              </Suspense>
            ) : (
              <div className="space-y-3">
                <p className="text-ui-meta text-ink-muted">
                  Files are saved in projects. Choose or create a project, then
                  open a conversation there to add files.
                </p>
                <Button
                  onClick={() => {
                    setView(null);
                    onBrowseProjects();
                  }}
                >
                  Choose a project
                </Button>
              </div>
            )}
          </>
        )}
      </Popover>
    </>
  );
}
