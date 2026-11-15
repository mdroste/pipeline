import { useCallback, useEffect, useRef, useState } from "react";
import ReportIssueLink from "./ReportIssueLink";
import IconButton from "../ui/IconButton";
import { Icon } from "../ui/icons";

interface ConfirmOptions {
  title?: string;
  confirmLabel?: string;
  /** Label for the dismiss button when "Cancel" would be ambiguous. */
  cancelLabel?: string;
  destructive?: boolean;
  /** A report to read, not a choice: only the confirm button is offered. */
  acknowledgeOnly?: boolean;
}

interface PromptOptions extends ConfirmOptions {
  /** Visible label for the text field; the message stays as its description. */
  label?: string;
  defaultValue?: string;
  placeholder?: string;
  /** Hide the typed text, for values such as API tokens. */
  concealed?: boolean;
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

const TOAST_DURATION = 6000;

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
  const dismissToast = useCallback(
    (id: number) =>
      setToasts((current) =>
        current.filter((candidate) => candidate.id !== id),
      ),
    [],
  );

  useEffect(() => {
    dialogHandler = (next) => setQueue((current) => [...current, next]);
    toastHandler = (toast) => {
      setToasts((current) => [...current.slice(-3), toast]);
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
        className="ui-overlay-frame m-auto max-h-[calc(100dvh-2rem)] w-[min(30rem,calc(100%-2rem))] overflow-auto p-0"
      >
        {request && (
          <form
            key={request.id}
            method="dialog"
            className="ui-overlay-body"
            onSubmit={(event) => {
              event.preventDefault();
              if (request.kind === "prompt") finish(true);
            }}
          >
            <h2
              id="pipeline-confirm-title"
              className="break-words text-ui-title font-semibold"
            >
              {request.title ||
                (request.kind === "prompt"
                  ? "Enter a value"
                  : "Please confirm")}
            </h2>
            <p
              id="pipeline-confirm-description"
              className="mt-3 whitespace-pre-line break-words text-sm leading-6 text-gray-600 dark:text-gray-300"
            >
              {request.message}
            </p>
            {request.kind === "prompt" && (
              <label className="mt-4 block text-sm font-medium">
                {request.label && <span>{request.label}</span>}
                <input
                  ref={inputRef}
                  type={request.concealed ? "password" : "text"}
                  autoComplete={request.concealed ? "off" : undefined}
                  aria-label={request.label ? undefined : request.message}
                  defaultValue={request.defaultValue ?? ""}
                  placeholder={request.placeholder}
                  className="ui-input mt-1 w-full font-normal"
                />
              </label>
            )}
            <div className="ui-overlay-footer">
              {!request.acknowledgeOnly && (
                <button
                  autoFocus={request.kind === "confirm"}
                  type="button"
                  onClick={() => finish(false)}
                  className="ui-button"
                >
                  {request.cancelLabel || "Cancel"}
                </button>
              )}
              <button
                autoFocus={request.acknowledgeOnly}
                type={request.kind === "prompt" ? "submit" : "button"}
                onClick={
                  request.kind === "confirm" ? () => finish(true) : undefined
                }
                className={`ui-button ${request.destructive ? "ui-button--danger" : "ui-button--primary"}`}
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
          <ToastCard key={toast.id} toast={toast} onDismiss={dismissToast} />
        ))}
      </div>
    </>
  );
}

/** Leaves on its own, but never while it is being read: the time starts over
 *  once the pointer and keyboard focus have left it. */
function ToastCard({
  toast,
  onDismiss,
}: {
  toast: Toast;
  onDismiss: (id: number) => void;
}) {
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const held = hovered || focused;
  useEffect(() => {
    if (held) return;
    const timer = window.setTimeout(() => onDismiss(toast.id), TOAST_DURATION);
    return () => window.clearTimeout(timer);
  }, [held, onDismiss, toast.id]);

  return (
    <div
      role={toast.kind === "error" ? "alert" : "status"}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onFocus={() => setFocused(true)}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget))
          setFocused(false);
      }}
      className={`ui-toast pointer-events-auto ${
        toast.kind === "error"
          ? "ui-toast--error"
          : toast.kind === "success"
            ? "ui-toast--success"
            : ""
      }`}
    >
      <div className="flex gap-3">
        <span className="min-w-0 flex-1 whitespace-pre-line break-words">
          {toast.message}
          {toast.kind === "error" && <ReportIssueLink />}
        </span>
        {toast.action && (
          <button
            type="button"
            className="pointer-events-auto underline"
            onClick={() => {
              // A click leaves focus here, which would hold the toast open.
              onDismiss(toast.id);
              toast.action?.run();
            }}
          >
            {toast.action.label}
          </button>
        )}
        <IconButton
          label="Dismiss notification"
          tooltip={null}
          size="sm"
          onClick={() => onDismiss(toast.id)}
        >
          <Icon name="close" className="h-4 w-4" />
        </IconButton>
      </div>
    </div>
  );
}
