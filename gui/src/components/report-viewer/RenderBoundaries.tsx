import React from "react";
import { MAX_RENDER_CHARS } from "./limits";

export class ReportRenderErrorBoundary extends React.Component<
  { markdown: string; children: React.ReactNode },
  { hasError: boolean }
> {
  state = { hasError: false };

  static getDerivedStateFromError() {
    return { hasError: true };
  }

  componentDidCatch(error: unknown) {
    console.error(
      "Report rendering failed, falling back to plain text:",
      error,
    );
  }

  componentDidUpdate(previous: { markdown: string }) {
    if (previous.markdown !== this.props.markdown && this.state.hasError) {
      this.setState({ hasError: false });
    }
  }

  render() {
    if (!this.state.hasError) return this.props.children;
    const truncated = this.props.markdown.length > MAX_RENDER_CHARS;
    return (
      <div className="h-full overflow-auto p-6">
        <div
          role="alert"
          className="mb-4 rounded border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-800 dark:border-amber-700 dark:bg-amber-950 dark:text-amber-200"
        >
          This response could not be formatted. Showing the saved text instead.
        </div>
        <pre className="whitespace-pre-wrap break-words text-xs leading-relaxed text-gray-800 dark:text-gray-200">
          {this.props.markdown.slice(0, MAX_RENDER_CHARS)}
          {truncated ? "\n\n[Plain-text preview truncated.]" : ""}
        </pre>
      </div>
    );
  }
}

/**
 * Catches render errors from rehype-katex/remark-math so malformed `$…$`
 * in LLM output can't crash the whole report view. On error, the children
 * render function is invoked again with `fallback=true` so the caller can
 * re-render without the math plugins. State resets when `resetKey` changes.
 */
export class MathErrorBoundary extends React.Component<
  { resetKey: string; children: (fallback: boolean) => React.ReactNode },
  { hasError: boolean }
> {
  state = { hasError: false };
  static getDerivedStateFromError() {
    return { hasError: true };
  }
  componentDidCatch(error: unknown) {
    console.error(
      "Math rendering failed, falling back to plain markdown:",
      error,
    );
  }
  componentDidUpdate(prev: { resetKey: string }) {
    if (prev.resetKey !== this.props.resetKey && this.state.hasError) {
      this.setState({ hasError: false });
    }
  }
  render() {
    return this.props.children(this.state.hasError);
  }
}
