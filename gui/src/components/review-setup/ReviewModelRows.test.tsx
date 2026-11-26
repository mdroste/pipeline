import { useState } from "react";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import ReviewModelRows from "./ReviewModelRows";
import { makeSettings } from "../../test/settingsFixtures";
import type { ModelCatalog, ModelSelection } from "../../lib/reviewModelTypes";

const catalog = {
  provider: "codex",
  transport: "cli",
  default_model: "model-a",
  roles: [],
  models: [
    {
      id: "model-a",
      display_name: "Model A",
      default_effort: "medium",
      supported_efforts: ["low", "medium", "high"],
    },
    {
      id: "model-b",
      display_name: "Model B",
      default_effort: "high",
      supported_efforts: ["low", "high"],
    },
  ],
} as unknown as ModelCatalog;

let saved: {
  modelOverrides: Record<string, ModelSelection>;
  effortOverrides: Record<string, string>;
};
function Harness({ live = catalog }: { live?: ModelCatalog }) {
  const [value, setValue] = useState({
    agents: ["codex"],
    modelOverrides: {} as Record<string, ModelSelection>,
    effortOverrides: {},
  });
  saved = value;
  return (
    <ReviewModelRows
      label="Team"
      team
      settings={{ ...makeSettings(), preferred_provider: "codex" }}
      catalogs={{ codex: live }}
      providers={["codex"]}
      {...value}
      onChange={setValue}
    />
  );
}

it("updates the compact default when changing models or refreshing the catalog without pinning it", async () => {
  const user = userEvent.setup();
  const { rerender } = render(<Harness />);
  const thinking = screen.getByLabelText("Team ChatGPT thinking");
  const model = screen.getByLabelText("Team ChatGPT model");
  expect(
    within(thinking).getByRole("option", { name: "Medium (default)" }),
  ).toHaveValue("");
  expect(thinking).toHaveAttribute("title", "ChatGPT default thinking: Medium");
  expect(model).toHaveDisplayValue("Model A (auto)");
  await user.selectOptions(model, "pinned:model-b");
  expect(thinking).toHaveDisplayValue("High (default)");
  expect(saved.effortOverrides).toEqual({});
  await user.selectOptions(thinking, "low");
  expect(saved.effortOverrides).toEqual({ "codex:cli": "low" });
  await user.selectOptions(thinking, "");
  await user.selectOptions(model, "inherit");
  expect(saved).toMatchObject({ modelOverrides: {}, effortOverrides: {} });
  rerender(<Harness live={{ ...catalog, default_model: "model-b" }} />);
  expect(model).toHaveDisplayValue("Model B (auto)");
  expect(thinking).toHaveDisplayValue("High (default)");
  expect(saved).toMatchObject({ modelOverrides: {}, effortOverrides: {} });
});
