import { useEffect, useRef, useState } from "react";

interface ConfirmOptions {
  title?: string;
  confirmLabel?: string;
  destructive?: boolean;
}

interface ConfirmRequest extends ConfirmOptions {
  message: string;
  resolve: (value: boolean) => void;
}

interface Toast {
  id: number;
  message: string;
  kind: "error" | "success" | "info";
}

let confirmHandler: ((request: ConfirmRequest) => void) | null = null;
let toastHandler: ((toast: Toast) => void) | null = null;
let nextToastId = 1;

export function confirmDialog(message: string, options: ConfirmOptions = {}): Promise<boolean> {
  if (!confirmHandler) return Promise.resolve(false);
  return new Promise((resolve) => confirmHandler?.({ message, resolve, ...options }));
}

export function notify(
  message: string,
  kind: Toast["kind"] = "error",
): void {
  toastHandler?.({ id: nextToastId++, message, kind });
}

export default function DialogService({ children }: { children: React.ReactNode }) {
  const [request, setRequest] = useState<ConfirmRequest | null>(null);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const dialogRef = useRef<HTMLDialogElement>(null);
  const priorFocus = useRef<HTMLElement | null>(null);

  useEffect(() => {
    confirmHandler = setRequest;
    toastHandler = (toast) => {
      setToasts((current) => [...current.slice(-3), toast]);
      window.setTimeout(() => {
        setToasts((current) => current.filter((candidate) => candidate.id !== toast.id));
      }, 6000);
    };
    return () => {
      confirmHandler = null;
      toastHandler = null;
    };
  }, []);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog || !request) return;
    priorFocus.current = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    if (!dialog.open) dialog.showModal();
  }, [request]);

  const finish = (result: boolean) => {
    const active = request;
    dialogRef.current?.close();
    setRequest(null);
    active?.resolve(result);
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
          <form method="dialog" className="p-6" onSubmit={(event) => event.preventDefault()}>
            <h2 id="pipeline-confirm-title" className="text-lg font-semibold">
              {request.title || "Please confirm"}
            </h2>
            <p id="pipeline-confirm-description" className="mt-3 whitespace-pre-line text-sm leading-6 text-gray-600 dark:text-gray-300">
              {request.message}
            </p>
            <div className="mt-6 flex justify-end gap-2">
              <button
                autoFocus
                type="button"
                onClick={() => finish(false)}
                className="rounded-lg border border-gray-300 px-3 py-2 text-sm font-medium dark:border-gray-700"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => finish(true)}
                className={`rounded-lg px-3 py-2 text-sm font-medium text-white ${
                  request.destructive ? "bg-red-600 hover:bg-red-700" : "bg-gray-900 hover:bg-gray-800 dark:bg-gray-100 dark:text-gray-900"
                }`}
              >
                {request.confirmLabel || "Continue"}
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
              <span className="min-w-0 flex-1 whitespace-pre-line">{toast.message}</span>
              <button
                type="button"
                aria-label="Dismiss notification"
                onClick={() => setToasts((current) => current.filter((candidate) => candidate.id !== toast.id))}
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
