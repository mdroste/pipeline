import { Component, ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
  resetKey: number;
}

export default class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, resetKey: 0 };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  render() {
    if (this.state.error) {
      return (
        <div className="h-screen flex items-center justify-center bg-gray-50 dark:bg-gray-950 p-8">
          <div className="max-w-md text-center">
            <h1 className="text-lg font-semibold text-red-600 dark:text-red-400 mb-2">
              Something went wrong
            </h1>
            <pre className="text-sm text-gray-600 dark:text-gray-400 bg-gray-100 dark:bg-gray-900 rounded-lg p-4 mb-4 text-left overflow-auto whitespace-pre-wrap max-h-64">
              {this.state.error.message}
              {this.state.error.stack && "\n\n" + this.state.error.stack}
            </pre>
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
