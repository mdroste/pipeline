import { useEffect, useRef, useState } from "react";

interface ConfirmOptions {
  title?: string;
  confirmLabel?: string;
  /** Label for the dismiss button when "Cancel" would be ambiguous. */
  cancelLabel?: string;
  destructive?: boolean;
}

interface PromptOptions extends ConfirmOptions {
  /** Visible label for the text field; the message stays as its description. */
  label?: string;
  defaultValue?: string;
  placeholder?: string;
}

type DialogRequest =
  | (ConfirmOptions & {
      kind: "confirm";
      id: number;
      message: string;
      resolve: (value: boolean) => void;
    })
  | (PromptOptions & {
      kind: "prompt";
      id: number;
      message: string;
      resolve: (value: string | null) => void;
    });

interface Toast {
  id: number;
  message: string;
  kind: "error" | "success" | "info";
  action?: { label: string; run: () => void };
}

let dialogHandler: ((request: DialogRequest) => void) | null = null;
let toastHandler: ((toast: Toast) => void) | null = null;
let nextToastId = 1;
let nextDialogId = 1;

export function confirmDialog(
  message: string,
  options: ConfirmOptions = {},
): Promise<boolean> {
  if (!dialogHandler) return Promise.resolve(false);
  return new Promise((resolve) =>
    dialogHandler?.({
      ...options,
      kind: "confirm",
      id: nextDialogId++,
      message,
      resolve,
    }),
  );
}

/**
 * In-app replacement for window.prompt, which returns null in the macOS
 * webview. Resolves to the entered text, or null when cancelled.
 */
export function promptDialog(
  message: string,
  options: PromptOptions = {},
): Promise<string | null> {
  if (!dialogHandler) return Promise.resolve(null);
  return new Promise((resolve) =>
    dialogHandler?.({
      ...options,
      kind: "prompt",
      id: nextDialogId++,
      message,
      resolve,
    }),
  );
}

export function notify(
  message: string,
  kind: Toast["kind"] = "error",
  action?: Toast["action"],
): void {
  toastHandler?.({ id: nextToastId++, message, kind, action });
}

export default function DialogService({
  children,
}: {
  children: React.ReactNode;
}) {
  // Requests wait in order so a second caller never strands the first one's
  // promise or replaces text the user is typing.
  const [queue, setQueue] = useState<DialogRequest[]>([]);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const dialogRef = useRef<HTMLDialogElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const priorFocus = useRef<HTMLElement | null>(null);
  const request = queue[0] ?? null;

  useEffect(() => {
    dialogHandler = (next) => setQueue((current) => [...current, next]);
    toastHandler = (toast) => {
      setToasts((current) => [...current.slice(-3), toast]);
      window.setTimeout(() => {
        setToasts((current) =>
          current.filter((candidate) => candidate.id !== toast.id),
        );
      }, 6000);
    };
    return () => {
      dialogHandler = null;
      toastHandler = null;
    };
  }, []);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog || !request) return;
    if (!dialog.open) {
      priorFocus.current =
        document.activeElement instanceof HTMLElement
          ? document.activeElement
          : null;
      dialog.showModal();
    }
    if (request.kind === "prompt") {
      inputRef.current?.focus();
      inputRef.current?.select();
    }
  }, [request]);

  const finish = (accepted: boolean) => {
    const active = request;
    if (!active) return;
    const text = inputRef.current?.value ?? "";
    setQueue((current) => (current[0] === active ? current.slice(1) : current));
    if (active.kind === "confirm") active.resolve(accepted);
    else active.resolve(accepted ? text : null);
    // The next queued request reuses the open dialog.
    if (queue.length > 1) return;
    dialogRef.current?.close();
    window.setTimeout(() => priorFocus.current?.focus(), 0);
  };

  return (
    <>
      {children}
      <dialog
        ref={dialogRef}
        aria-labelledby="pipeline-confirm-title"
        aria-describedby="pipeline-confirm-description"
        onCancel={(event) => {
          event.preventDefault();
          finish(false);
        }}
        className="m-auto w-[min(30rem,calc(100%-2rem))] rounded-xl border border-gray-200 bg-white p-0 text-gray-900 shadow-2xl backdrop:bg-black/45 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-100"
      >
        {request && (
          <form
            key={request.id}
            method="dialog"
            className="p-6"
            onSubmit={(event) => {
              event.preventDefault();
              if (request.kind === "prompt") finish(true);
            }}
          >
            <h2 id="pipeline-confirm-title" className="text-lg font-semibold">
              {request.title ||
                (request.kind === "prompt"
                  ? "Enter a value"
                  : "Please confirm")}
            </h2>
            <p
              id="pipeline-confirm-description"
              className="mt-3 whitespace-pre-line text-sm leading-6 text-gray-600 dark:text-gray-300"
            >
              {request.message}
            </p>
            {request.kind === "prompt" && (
              <label className="mt-4 block text-sm font-medium">
                {request.label && <span>{request.label}</span>}
                <input
                  ref={inputRef}
                  aria-label={request.label ? undefined : request.message}
                  defaultValue={request.defaultValue ?? ""}
                  placeholder={request.placeholder}
                  className="mt-1 w-full rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm font-normal dark:border-gray-700 dark:bg-gray-950"
                />
              </label>
            )}
            <div className="mt-6 flex justify-end gap-2">
              <button
                autoFocus={request.kind === "confirm"}
                type="button"
                onClick={() => finish(false)}
                className="rounded-lg border border-gray-300 px-3 py-2 text-sm font-medium dark:border-gray-700"
              >
                {request.cancelLabel || "Cancel"}
              </button>
              <button
                type={request.kind === "prompt" ? "submit" : "button"}
                onClick={
                  request.kind === "confirm" ? () => finish(true) : undefined
                }
                className={`rounded-lg px-3 py-2 text-sm font-medium text-white ${
                  request.destructive
                    ? "bg-red-600 hover:bg-red-700"
                    : "bg-gray-900 hover:bg-gray-800 dark:bg-gray-100 dark:text-gray-900"
                }`}
              >
                {request.confirmLabel ||
                  (request.kind === "prompt" ? "OK" : "Continue")}
              </button>
            </div>
          </form>
        )}
      </dialog>
      <div
        aria-live="polite"
        aria-atomic="false"
        className="pointer-events-none fixed right-4 top-4 z-[100] flex w-[min(24rem,calc(100%-2rem))] flex-col gap-2"
      >
        {toasts.map((toast) => (
          <div
            key={toast.id}
            role={toast.kind === "error" ? "alert" : "status"}
            className={`pointer-events-auto rounded-lg border px-4 py-3 text-sm shadow-lg ${
              toast.kind === "error"
                ? "border-red-200 bg-red-50 text-red-800 dark:border-red-900 dark:bg-red-950 dark:text-red-200"
                : toast.kind === "success"
                  ? "border-emerald-200 bg-emerald-50 text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950 dark:text-emerald-200"
                  : "border-gray-200 bg-white text-gray-800 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200"
            }`}
          >
            <div className="flex gap-3">
              <span className="min-w-0 flex-1 whitespace-pre-line">
                {toast.message}
              </span>
              {toast.action && (
                <button
                  type="button"
                  className="pointer-events-auto underline"
                  onClick={toast.action.run}
                >
                  {toast.action.label}
                </button>
              )}
              <button
                type="button"
                aria-label="Dismiss notification"
                onClick={() =>
                  setToasts((current) =>
                    current.filter((candidate) => candidate.id !== toast.id),
                  )
                }
                className="pointer-events-auto shrink-0 rounded px-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-current"
              >
                ×
              </button>
            </div>
          </div>
        ))}
      </div>
    </>
  );
}
