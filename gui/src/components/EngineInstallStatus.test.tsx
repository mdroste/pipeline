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
    expect(screen.queryByText("Graphics card support")).toBeNull();
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
    expect(screen.getByText("Graphics card support")).toBeTruthy();
    expect(screen.queryByText("Python")).toBeNull();
    expect(screen.queryByText("Layout model")).toBeNull();
  });

  it("keeps completed parser steps when default graphics setup follows", () => {
    render(
      <EngineInstallStatus
        active
        phases={{
          runtime: "done",
          packages: "done",
          models: "done",
          gpu: "running",
        }}
        download={null}
        logLines={[]}
      />,
    );
    expect(screen.getByText("Python")).toBeTruthy();
    expect(screen.getByText("Layout model")).toBeTruthy();
    expect(screen.getByText("Graphics card support")).toBeTruthy();
    expect(screen.getByText("Setting up graphics card support…")).toBeTruthy();
  });
});
