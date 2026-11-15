import { useState } from "react";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import ManuscriptTexTools from "./ManuscriptTexTools";
import { initialBuild } from "./ManuscriptBuildSheet";

const detect = vi.hoisted(() => vi.fn());
vi.mock("../../lib/workbench/executionClient", () => ({
  executionClient: { detectToolchains: detect },
}));
function Harness({ saved = false }: { saved?: boolean }) {
  const [config, setConfig] = useState(initialBuild);
  return (
    <>
      <span data-testid="engine">{config.engine}</span>
      <ManuscriptTexTools
        workspaceId="ws"
        config={config}
        setConfig={setConfig}
        saved={saved}
      />
    </>
  );
}
beforeEach(() => {
  detect.mockReset();
});

it("selects an available engine when latexmk is absent", async () => {
  detect.mockResolvedValue({
    tools: [{ adapter: "latex", path: "/TeX/pdflatex", version: "2026" }],
  });
  render(<Harness />);
  await waitFor(() =>
    expect(screen.getByTestId("engine")).toHaveTextContent("pdflatex"),
  );
  expect(screen.getByLabelText("TeX executable path (optional)")).toHaveValue(
    "/TeX/pdflatex",
  );
});

it("does not replace a manual executable while detection is in flight", async () => {
  let finish!: (value: unknown) => void;
  detect.mockReturnValue(
    new Promise((resolve) => {
      finish = resolve;
    }),
  );
  render(<Harness />);
  fireEvent.change(screen.getByLabelText("TeX executable path (optional)"), {
    target: { value: "/custom/latexmk" },
  });
  await act(async () =>
    finish({ tools: [{ adapter: "latex", path: "/TeX/pdflatex" }] }),
  );
  expect(screen.getByTestId("engine")).toHaveTextContent("latexmk");
  expect(screen.getByLabelText("TeX executable path (optional)")).toHaveValue(
    "/custom/latexmk",
  );
});

it("keeps saved engine choices even when detection finds another engine", async () => {
  detect.mockResolvedValue({
    tools: [{ adapter: "latex", path: "/TeX/pdflatex" }],
  });
  render(<Harness saved />);
  await screen.findByText("Installed TeX tools");
  expect(screen.getByTestId("engine")).toHaveTextContent("latexmk");
});
