import { useEffect, useState, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

// The Windows caption buttons, drawn by the app because the window is
// undecorated. Close goes through the same close-requested path as a native
// close button, so unsaved-work prompts and background Automations still apply.
// Sits above dialogs, as native caption buttons would.

const BUTTON_CLASS =
  "inline-flex h-8 w-[46px] items-center justify-center text-gray-700 transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent dark:text-neutral-300";
const NEUTRAL_HOVER =
  "hover:bg-black/5 active:bg-black/10 dark:hover:bg-white/10 dark:active:bg-white/15";
const CLOSE_HOVER =
  "hover:bg-[#c42b1c] hover:text-white active:bg-[#b3271a] active:text-white dark:hover:text-white";

function Glyph({ children }: { children: ReactNode }) {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 10 10"
      className="h-2.5 w-2.5"
      fill="none"
      stroke="currentColor"
      strokeWidth="1"
      shapeRendering="crispEdges"
    >
      {children}
    </svg>
  );
}

function CaptionButton({
  label,
  className,
  onClick,
  children,
}: {
  label: string;
  className: string;
  onClick: () => Promise<void>;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className={`${BUTTON_CLASS} ${className}`}
      onClick={() => {
        onClick().catch((error) =>
          console.warn(`Unable to ${label.toLowerCase()} window:`, error),
        );
      }}
    >
      {children}
    </button>
  );
}

export default function WindowControls() {
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    const appWindow = getCurrentWindow();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const syncMaximized = () => {
      appWindow
        .isMaximized()
        .then((next) => {
          if (!disposed) setMaximized(next);
        })
        .catch((error) => console.warn("Unable to read window state:", error));
    };
    syncMaximized();
    appWindow
      .onResized(syncMaximized)
      .then((stopListening) => {
        if (disposed) {
          stopListening();
        } else {
          unlisten = stopListening;
        }
      })
      .catch((error) => console.warn("Unable to watch window size:", error));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  return (
    <div
      role="group"
      aria-label="Window controls"
      className="fixed right-0 top-0 z-[130] flex"
    >
      <CaptionButton
        label="Minimize"
        className={NEUTRAL_HOVER}
        onClick={() => getCurrentWindow().minimize()}
      >
        <Glyph>
          <path d="M0 5.5h10" />
        </Glyph>
      </CaptionButton>
      <CaptionButton
        label={maximized ? "Restore" : "Maximize"}
        className={NEUTRAL_HOVER}
        onClick={() => getCurrentWindow().toggleMaximize()}
      >
        <Glyph>
          {maximized ? (
            <path d="M0.5 2.5h7v7h-7zM2.5 2.5v-2h7v7h-2" />
          ) : (
            <path d="M0.5 0.5h9v9h-9z" />
          )}
        </Glyph>
      </CaptionButton>
      <CaptionButton
        label="Close"
        className={CLOSE_HOVER}
        onClick={() => getCurrentWindow().close()}
      >
        <Glyph>
          <path d="M0.5 0.5l9 9M9.5 0.5l-9 9" shapeRendering="auto" />
        </Glyph>
      </CaptionButton>
    </div>
  );
}
