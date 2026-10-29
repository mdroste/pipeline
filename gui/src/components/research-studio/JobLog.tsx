import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { studioClient, type JobStatus } from "../../lib/studioClient";
import { button } from "./shared";

export const ACTIVE_JOB_STATES = ["queued", "running", "cancelling"];
/** Characters kept on screen. The saved log can be longer. */
const DISPLAY_LIMIT = 200_000;
/** Bytes the backend saves for each output stream. */
const SAVED_LIMIT = 4 * 1024 * 1024;
const POLL_MS = 750;
/** Distance from the bottom, in pixels, that still counts as following. */
const FOLLOW_SLACK = 16;

type Stream = "stdout" | "stderr";
// Same labels as a finished run's output in the research panel.
const STREAMS: readonly [Stream, string][] = [
  ["stdout", "Standard output"],
  ["stderr", "Errors and warnings"],
];
interface LogState {
  key: string;
  text: string;
  offset: number;
  /** Older output was dropped from the on-screen copy. */
  trimmed: boolean;
  /** A running job's capture of this stream reached the saved limit. */
  limitReached: boolean;
  /** The job has ended and every saved byte has been read. */
  complete: boolean;
}
const emptyLog = (key: string): LogState => ({
  key,
  text: "",
  offset: 0,
  trimmed: false,
  limitReached: false,
  complete: false,
});

/**
 * Incremental view of one job's output stream. Reading pauses while the panel
 * is hidden or collapsed and resumes from the last offset, and stops once an
 * ended job has been read to the end. The view follows new output while it
 * is scrolled to the bottom.
 */
export default function JobLog({
  workspaceId,
  executionId,
  job,
  live,
  onError,
}: {
  workspaceId: string;
  executionId: string;
  job: JobStatus | undefined;
  /** The panel is visible and expanded. */
  live: boolean;
  onError: (message: string) => void;
}) {
  const [stream, setStream] = useState<Stream>("stdout");
  const key = `${workspaceId}\n${executionId}\n${stream}`;
  const [log, setLog] = useState(() => emptyLog(key));
  const logRef = useRef(log);
  const ended = !!job && !ACTIVE_JOB_STATES.includes(job.state);
  const endedRef = useRef(ended);
  endedRef.current = ended;
  const errorRef = useRef(onError);
  errorRef.current = onError;
  const pre = useRef<HTMLPreElement>(null);
  const follow = useRef(true);

  useEffect(() => {
    if (logRef.current.key !== key) {
      logRef.current = emptyLog(key);
      setLog(logRef.current);
      follow.current = true;
    }
    if (!live || logRef.current.complete) return;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    const read = async () => {
      // Only an empty read that began after the job ended proves the end.
      const endedBefore = endedRef.current;
      const current = logRef.current;
      try {
        const page = await studioClient.log(
          workspaceId,
          executionId,
          stream,
          current.offset,
        );
        if (disposed) return;
        const text = current.text + page.text;
        const next: LogState = {
          key,
          text: text.slice(-DISPLAY_LIMIT),
          offset: page.nextOffset,
          trimmed: current.trimmed || text.length > DISPLAY_LIMIT,
          limitReached:
            current.limitReached ||
            (page.truncated && page.nextOffset >= SAVED_LIMIT),
          complete: endedBefore && page.nextOffset === current.offset,
        };
        logRef.current = next;
        if (page.text || next.complete || next.offset !== current.offset) {
          setLog(next);
        }
        if (next.complete) return;
      } catch (e) {
        if (disposed) return;
        errorRef.current(String(e));
      }
      timer = setTimeout(() => void read(), POLL_MS);
    };
    void read();
    return () => {
      disposed = true;
      clearTimeout(timer);
    };
  }, [live, key, workspaceId, executionId, stream]);

  const shown = log.key === key ? log : emptyLog(key);
  useLayoutEffect(() => {
    const element = pre.current;
    if (element && follow.current) element.scrollTop = element.scrollHeight;
  }, [shown.text, live]);

  // The saved receipt records truncation per stream once the job has ended.
  const savedFlag = job?.execution.validation[`${stream}Truncated`];
  const limitReached =
    ended && typeof savedFlag === "boolean" ? savedFlag : shown.limitReached;
  return (
    <>
      <div className="flex gap-2">
        {STREAMS.map(([s, label]) => (
          <button
            className={button}
            key={s}
            aria-pressed={stream === s}
            onClick={() => setStream(s)}
          >
            {label}
          </button>
        ))}
      </div>
      <pre
        ref={pre}
        aria-label="Job log"
        onScroll={(event) => {
          const element = event.currentTarget;
          follow.current =
            element.scrollHeight - element.scrollTop - element.clientHeight <=
            FOLLOW_SLACK;
        }}
        className="max-h-64 overflow-auto whitespace-pre-wrap break-words rounded bg-neutral-950 p-3 text-xs text-neutral-100"
      >
        {shown.text || (shown.complete ? "No output." : "No output yet.")}
      </pre>
      {shown.trimmed && (
        <p className="text-xs">Showing the last 200,000 characters.</p>
      )}
      {limitReached && (
        <p className="text-xs">
          Log limit reached. Up to 4 MiB is saved for each output stream.
        </p>
      )}
    </>
  );
}
