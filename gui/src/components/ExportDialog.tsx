import type { ReactNode } from "react";
import { createPortal } from "react-dom";
import useModalDialog from "../hooks/useModalDialog";
import IconButton from "../ui/IconButton";
import { Icon } from "../ui/icons";

export default function ExportDialog({
  onClose,
  children,
}: {
  onClose: () => void;
  children: ReactNode;
}) {
  const dialog = useModalDialog<HTMLDivElement>(onClose, true, true);
  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-scrim p-4">
      <div
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-label="Export report"
        tabIndex={-1}
        className="ui-overlay-frame flex max-h-[85vh] w-full max-w-2xl flex-col"
      >
        <header className="ui-overlay-header">
          <h2 className="text-ui-title font-semibold">Export report</h2>
          <IconButton label="Close export" onClick={onClose}>
            <Icon name="close" className="h-4 w-4" />
          </IconButton>
        </header>
        <div className="ui-overlay-body min-h-0 overflow-auto">{children}</div>
      </div>
    </div>,
    document.body,
  );
}
