import { memo, useMemo } from "react";
import ReactMarkdown from "react-markdown";
import remarkMath from "remark-math";
import remarkGfm from "remark-gfm";
import rehypeKatex from "rehype-katex";
import "katex/dist/katex.min.css";
import {
  fileMarkdownComponents,
  fileMarkdownUrlTransform,
} from "./file-workspace/markdownComponents";
import WorkspaceMessageActions from "./WorkspaceMessageActions";
import WorkspaceIcon from "./WorkspaceIcon";

// Module-level plugin lists keep the Markdown options identical between renders.
const REMARK_PLUGINS = [remarkGfm, remarkMath];
const REHYPE_PLUGINS = [rehypeKatex];

/** Markdown, math and file links for one message; parsed again only when its text changes. */
export const TranscriptMarkdown = memo(function TranscriptMarkdown({
  text,
  messageId,
}: {
  text: string;
  /** Keeps this message's footnotes apart from those of other messages. */
  messageId?: string;
}) {
  const options = useMemo(
    () => (messageId ? { clobberPrefix: `message-${messageId}-` } : undefined),
    [messageId],
  );
  return (
    <ReactMarkdown
      urlTransform={fileMarkdownUrlTransform}
      remarkPlugins={REMARK_PLUGINS}
      rehypePlugins={REHYPE_PLUGINS}
      remarkRehypeOptions={options}
      components={fileMarkdownComponents}
    >
      {text}
    </ReactMarkdown>
  );
});

/**
 * One saved transcript message. Memoized so that typing in the composer or a
 * streaming response does not re-render every message in the window.
 */
const TranscriptMessage = memo(function TranscriptMessage({
  id,
  text,
  role,
  selected,
}: {
  id: string;
  text: string;
  role: string;
  selected: boolean;
}) {
  const user = role === "You";
  return (
    <article
      id={`workspace-message-${id}`}
      tabIndex={-1}
      aria-label={`${role} message`}
      className={`workspace-message workspace-message--${user ? "user" : "assistant"}${selected ? " ring-2 ring-blue-300 ring-offset-4 dark:ring-offset-neutral-950" : ""}`}
    >
      <div className="workspace-message-author">
        {!user && (
          <span className="workspace-assistant-mark">
            <WorkspaceIcon name="message" size={14} />
          </span>
        )}
        {role}
      </div>
      <div className="workspace-message-body">
        <TranscriptMarkdown text={text} messageId={id} />
      </div>
      <WorkspaceMessageActions
        text={text}
        label={user ? "prompt" : "response"}
      />
    </article>
  );
});
export default TranscriptMessage;
