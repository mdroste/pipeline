import type { ReactNode } from "react";
import { createPortal } from "react-dom";
import useModalDialog from "../hooks/useModalDialog";

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
        className="flex max-h-[85vh] w-full max-w-2xl flex-col rounded-xl border border-line bg-raised text-ink shadow-ui-3"
      >
        <header className="flex items-center justify-between border-b border-line p-5">
          <h2 className="text-lg font-semibold">Export report</h2>
          <button
            aria-label="Close export"
            onClick={onClose}
            className="rounded px-2 py-1"
          >
            ✕
          </button>
        </header>
        <div className="min-h-0 overflow-auto p-5">{children}</div>
      </div>
    </div>,
    document.body,
  );
}
