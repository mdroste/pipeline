import { Component, type ReactNode } from "react";
export default class ResearchReaderBoundary extends Component<
  { children: ReactNode },
  { failed: boolean }
> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  render() {
    return this.state.failed ? (
      <div role="alert" className="m-4 rounded border p-4 text-sm">
        This reader could not display the object. The conversation and other
        research tools remain available.{" "}
        <button
          className="underline"
          onClick={() => this.setState({ failed: false })}
        >
          Retry reader
        </button>
      </div>
    ) : (
      this.props.children
    );
  }
}
