import { Component, Fragment, ReactNode } from "react";
import ReportIssueLink from "./ReportIssueLink";
import WindowControls from "./WindowControls";
import { isWindows } from "../lib/platform";

interface Props {
  children: ReactNode;
  /** Fill the enclosing pane instead of the window (boundaries inside the shell). */
  nested?: boolean;
  /** Allows focused tests to disable details that are available only in development builds. */
  showTechnicalDetails?: boolean;
}

interface State {
  error: Error | null;
  resetKey: number;
}

function publicErrorMessage(error: Error): string {
  const firstLine = error.message.split(/\r?\n/, 1)[0].trim();
  if (!firstLine) return "An unexpected interface error occurred.";
  return firstLine.length > 300 ? `${firstLine.slice(0, 297)}…` : firstLine;
}

export default class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, resetKey: 0 };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  render() {
    if (this.state.error) {
      const showTechnicalDetails =
        import.meta.env.DEV && this.props.showTechnicalDetails !== false;
      return (
        <div
          className={`${this.props.nested ? "h-full min-h-0 overflow-auto" : "h-screen"} flex bg-sunken p-8`}
        >
          {/* This screen replaces the app, and Windows has no native title bar
              to fall back on: keep the window movable and closable. */}
          {isWindows && !this.props.nested && (
            <>
              <div
                data-tauri-drag-region
                className="fixed inset-x-0 top-0 h-8"
              />
              <WindowControls />
            </>
          )}
          {/* Auto margins center the message without clipping a tall one. */}
          <div role="alert" className="m-auto max-w-md text-center">
            <h1 className="mb-2 text-xl font-semibold text-danger">
              Something went wrong
            </h1>
            <p className="text-sm text-gray-700 dark:text-gray-300 mb-3">
              {publicErrorMessage(this.state.error)}
            </p>
            <p className="text-xs text-gray-600 dark:text-gray-400 mb-4">
              Try again to reset the interface. If the problem continues,
              restart Pipeline.
              <ReportIssueLink />
            </p>
            {showTechnicalDetails && this.state.error.stack && (
              <details className="mb-4 text-left">
                <summary className="cursor-pointer text-xs font-medium text-gray-600 dark:text-gray-400">
                  Technical details
                </summary>
                <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap rounded-lg bg-gray-100 p-4 text-xs text-gray-600 dark:bg-gray-900 dark:text-gray-400">
                  {this.state.error.stack}
                </pre>
              </details>
            )}
            <button
              onClick={() =>
                this.setState((prev) => ({
                  error: null,
                  resetKey: prev.resetKey + 1,
                }))
              }
              className="ui-button ui-button--primary"
            >
              Try Again
            </button>
          </div>
        </div>
      );
    }

    // The key remounts children on retry, clearing corrupted state. A Fragment
    // adds no layout box, so children keep their parent's height chain.
    return <Fragment key={this.state.resetKey}>{this.props.children}</Fragment>;
  }
}
