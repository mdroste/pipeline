import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useEffect } from "react";
import { describe, expect, it, vi } from "vitest";

import type { EngineStatus } from "../../lib/types";
import { paddleEngine, makeSettings } from "../../test/settingsFixtures";
import { ExtractionSection } from "./PdfExtractionSettings";

let engines: EngineStatus[] = [];

vi.mock("../EnginesPanel", () => ({
  default: ({
    onEngineStatusChange,
  }: {
    onEngineStatusChange?: (engines: EngineStatus[]) => void;
  }) => {
    useEffect(() => onEngineStatusChange?.(engines), [onEngineStatusChange]);
    return null;
  },
}));

describe("processor cores setting", () => {
  it("stays hidden where pages are read with llama.cpp", () => {
    engines = [paddleEngine({ installed: true })];
    render(
      <ExtractionSection settings={makeSettings()} setSettings={vi.fn()} />,
    );
    expect(screen.queryByLabelText("PaddleOCR-VL processor cores")).toBeNull();
  });

  it("saves the chosen number of cores", async () => {
    engines = [
      paddleEngine({ installed: true }),
      paddleEngine({ id: "paddleocr-vl-gpu" }),
    ];
    const setSettings = vi.fn();
    const settings = makeSettings();
    render(<ExtractionSection settings={settings} setSettings={setSettings} />);

    await userEvent.click(screen.getByText("Advanced parser settings"));
    await userEvent.selectOptions(
      screen.getByLabelText("PaddleOCR-VL processor cores"),
      "8",
    );

    expect(setSettings).toHaveBeenCalledWith({
      ...settings,
      paddle_cpu_threads: 8,
    });
  });
});

describe("graphics card setting", () => {
  it("stays hidden until graphics card recognition is installed", () => {
    engines = [paddleEngine({ installed: true })];
    render(
      <ExtractionSection settings={makeSettings()} setSettings={vi.fn()} />,
    );
    expect(screen.queryByText("Use graphics card")).toBeNull();
  });

  it("turns graphics card recognition off", async () => {
    engines = [
      paddleEngine({ installed: true }),
      paddleEngine({ id: "paddleocr-vl-gpu", installed: true }),
    ];
    const setSettings = vi.fn();
    const settings = makeSettings();
    render(<ExtractionSection settings={settings} setSettings={setSettings} />);

    await userEvent.click(screen.getByText("Advanced parser settings"));
    await userEvent.click(
      screen.getByRole("switch", { name: "Use graphics card" }),
    );

    expect(setSettings).toHaveBeenCalledWith({
      ...settings,
      paddle_use_gpu: false,
    });
  });
});
