import { useState } from "react";
import { open as openUrl } from "@tauri-apps/plugin-shell";

export const ISSUES_URL = "https://github.com/mdroste/pipeline/issues";

/** Tells a researcher where to report a problem. It goes inside an error
 * message, takes that message's color, and starts on its own line. The address
 * never carries the error text, which can name private files. */
export default function ReportIssueLink() {
  const [openFailed, setOpenFailed] = useState(false);

  return (
    <span className="mt-1 block text-xs font-normal">
      {openFailed ? (
        "Pipeline could not open your browser. To report this, go to github.com/mdroste/pipeline/issues."
      ) : (
        <>
          If this looks like a bug, you can report it with this message on{" "}
          <a
            href={ISSUES_URL}
            onClick={(event) => {
              event.preventDefault();
              void openUrl(ISSUES_URL).catch(() => setOpenFailed(true));
            }}
            className="cursor-pointer underline underline-offset-2"
          >
            GitHub Issues
          </a>
          .
        </>
      )}
    </span>
  );
}
