import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import type { ModelCatalog, Settings } from "../../lib/types";
import ModelStage from "./ModelStage";

it("shows a role by its name", () => {
  render(
    <ModelStage
      title="Parallel review"
      agents={["claude"]}
      models={{ "claude:cli": { mode: "role", role: "quality" } }}
      settings={{ claude_access_mode: "subscription" } as Settings}
      catalogs={{
        claude: {
          models: [],
          roles: [
            { id: "quality", label: "Quality", description: "", model: "opus" },
          ],
        } as unknown as ModelCatalog,
      }}
    >
      <div />
    </ModelStage>,
  );

  expect(
    screen.getByText("Claude · Quality · Default thinking"),
  ).toBeInTheDocument();
});

it("names what an inheriting stage follows", () => {
  render(
    <ModelStage
      title="Input assessment"
      agents={["claude"]}
      inherited="Default model"
      settings={{ claude_access_mode: "subscription" } as Settings}
      catalogs={{}}
    >
      <div />
    </ModelStage>,
  );

  expect(screen.getByText("Default model")).toBeInTheDocument();
  expect(screen.queryByText(/Provider default/)).not.toBeInTheDocument();
});
