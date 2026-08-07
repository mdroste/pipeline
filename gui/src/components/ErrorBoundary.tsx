import { Component, ReactNode } from "react";

interface Props {
  children: ReactNode;
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
        <div className="h-screen flex items-center justify-center bg-gray-50 dark:bg-gray-950 p-8">
          <div role="alert" className="max-w-md text-center">
            <h1 className="text-lg font-semibold text-red-600 dark:text-red-400 mb-2">
              Something went wrong
            </h1>
            <p className="text-sm text-gray-700 dark:text-gray-300 mb-3">
              {publicErrorMessage(this.state.error)}
            </p>
            <p className="text-xs text-gray-600 dark:text-gray-400 mb-4">
              Try again to reset the interface. If the problem continues, restart
              Pipeline and include the message above when reporting it.
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
              onClick={() => this.setState((prev) => ({ error: null, resetKey: prev.resetKey + 1 }))}
              className="px-4 py-2 bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 rounded-lg text-sm font-medium hover:bg-gray-800 dark:hover:bg-gray-200 transition-colors"
            >
              Try Again
            </button>
          </div>
        </div>
      );
    }

    // Use key to force remount children on retry, clearing corrupted state
    return <div key={this.state.resetKey}>{this.props.children}</div>;
  }
}
