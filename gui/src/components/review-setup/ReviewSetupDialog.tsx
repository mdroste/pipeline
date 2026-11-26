import { useId, type ReactNode } from "react";
import { createPortal } from "react-dom";
import useModalDialog from "../../hooks/useModalDialog";
import { Icon } from "../../ui/icons";

export default function ReviewSetupDialog({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const id = useId();
  const ref = useModalDialog<HTMLDivElement>(onClose, true, true);
  return createPortal(
    <div className="review-dialog-backdrop">
      <div
        className="review-dialog"
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-labelledby={id}
        tabIndex={-1}
      >
        <header>
          <h2 id={id}>{title}</h2>
          <button
            type="button"
            className="ui-button"
            onClick={onClose}
            aria-label={`Close ${title}`}
          >
            <Icon name="close" />
          </button>
        </header>
        <div className="review-dialog-content">{children}</div>
      </div>
    </div>,
    document.body,
  );
}
