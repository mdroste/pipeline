import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import EngineInstallStatus from "./EngineInstallStatus";

describe("EngineInstallStatus", () => {
  it("shows the parser's steps", () => {
    render(
      <EngineInstallStatus
        active
        phases={{ runtime: "running" }}
        download={null}
        logLines={[]}
      />,
    );
    expect(screen.getByText("Python")).toBeTruthy();
    expect(screen.queryByText("Graphics card model")).toBeNull();
  });

  it("shows only its own step for graphics card recognition", () => {
    render(
      <EngineInstallStatus
        active
        phases={{ gpu: "running" }}
        download={null}
        logLines={[]}
      />,
    );
    expect(screen.getByText("Graphics card model")).toBeTruthy();
    expect(screen.queryByText("Python")).toBeNull();
    expect(screen.queryByText("Layout model")).toBeNull();
  });
});
