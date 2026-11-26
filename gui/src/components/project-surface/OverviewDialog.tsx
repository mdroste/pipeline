import { useProjectEnvironment } from "../../lib/projectEnvironment";
import { useEffect, useId, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import useModalDialog from "../../hooks/useModalDialog";
import WorkspaceIcon from "../WorkspaceIcon";

/** Keep local drafts when a context panel closes; hidden panels never trap focus. */
export default function OverviewDialog({
  title,
  open,
  onClose,
  children,
  error,
  onDismissError,
  busy = false,
}: {
  title: string;
  open: boolean;
  onClose: () => void;
  children: ReactNode;
  error?: string | null;
  onDismissError?: () => void;
  busy?: boolean;
}) {
  const { embeddedDialogs } = useProjectEnvironment();
  const heading = useId();
  const visited = useRef(false);
  const dialog = useModalDialog<HTMLDivElement>(
    onClose,
    open && !embeddedDialogs,
    true,
  );
  useEffect(() => {
    if (open)
      dialog.current?.querySelector<HTMLButtonElement>("button")?.focus();
  }, [open, title, dialog]);
  if (open) visited.current = true;
  if (!visited.current) return null;
  const content = (
    <div
      ref={dialog}
      hidden={!open}
      role={embeddedDialogs ? "group" : "dialog"}
      aria-modal={embeddedDialogs ? undefined : true}
      aria-labelledby={heading}
      tabIndex={-1}
      className="overview-dialog-backdrop"
    >
      <div className="overview-dialog ui-overlay-frame">
        <header className="overview-dialog-heading">
          <h2 id={heading}>{title}</h2>
          <button
            type="button"
            className="ui-button"
            aria-label="Close panel"
            onClick={onClose}
          >
            <WorkspaceIcon name="close" />
          </button>
        </header>
        {error && (
          <div role="alert" className="overview-dialog-error">
            <span>{error}</span>
            {onDismissError && (
              <button
                type="button"
                className="ui-button"
                onClick={onDismissError}
              >
                Dismiss
              </button>
            )}
          </div>
        )}
        <fieldset
          disabled={busy}
          aria-busy={busy}
          className="overview-dialog-body min-w-0"
        >
          {children}
        </fieldset>
      </div>
    </div>
  );
  return embeddedDialogs ? content : createPortal(content, document.body);
}

export function OverviewPane({
  open,
  children,
}: {
  open: boolean;
  children: ReactNode;
}) {
  const visited = useRef(false);
  if (open) visited.current = true;
  return visited.current ? <div hidden={!open}>{children}</div> : null;
}
