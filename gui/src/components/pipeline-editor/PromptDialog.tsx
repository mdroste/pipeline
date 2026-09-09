import { memo, useId, useState } from "react";
import useModalDialog from "../../hooks/useModalDialog";

function PromptDialog({
  title,
  defaultValue,
  onSubmit,
  onCancel,
}: {
  title: string;
  defaultValue: string;
  onSubmit: (value: string) => void;
  onCancel: () => void;
}) {
  const [value, setValue] = useState(defaultValue);
  const titleId = useId();
  const inputId = useId();
  const dialogRef = useModalDialog<HTMLDivElement>(onCancel);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        className="bg-white dark:bg-gray-800 rounded-xl shadow-xl w-80 p-5"
      >
        <p
          id={titleId}
          className="text-sm font-medium text-gray-900 dark:text-gray-100 mb-3"
        >
          {title}
        </p>
        <label htmlFor={inputId} className="sr-only">
          {title}
        </label>
        <input
          id={inputId}
          data-autofocus
          type="text"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && value.trim()) onSubmit(value.trim());
            if (e.key === "Escape") onCancel();
          }}
          autoFocus
          className="w-full py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                     text-gray-900 bg-white dark:bg-gray-700 dark:text-gray-200
                     focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent transition-colors"
        />
        <div className="flex justify-end gap-2 mt-4">
          <button
            type="button"
            onClick={onCancel}
            className="py-1.5 px-3 text-sm text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => {
              if (value.trim()) onSubmit(value.trim());
            }}
            disabled={!value.trim()}
            className="py-1.5 px-3 text-sm font-medium text-white bg-gray-900 dark:bg-gray-100 dark:text-gray-900 rounded-lg
                       hover:bg-gray-800 dark:hover:bg-gray-200 disabled:bg-gray-300 dark:disabled:bg-gray-600 transition-colors"
          >
            OK
          </button>
        </div>
      </div>
    </div>
  );
}

export default memo(PromptDialog);
