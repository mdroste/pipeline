import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import schemaDialect from "../../../tests/fixtures/workflow/schema-dialect.json";
import type { StepConfig } from "../../lib/types";
import SchemaEditorPanel from "./SchemaEditorPanel";
import { outputSchemaError } from "./utils";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const step = (patch: Partial<StepConfig>) =>
  ({
    id: "synthesis",
    label: "Synthesis",
    enabled: true,
    phase: "sequential",
    tools: [],
    agents: [],
    prompt: "Synthesize.",
    context: { include: [] },
    ...patch,
  }) as StepConfig;

function panel(target: StepConfig) {
  const onStepSchemaChange = vi.fn();
  const onDraftValidityChange = vi.fn();
  render(
    <SchemaEditorPanel
      target={{ kind: "step", step: target, publishesFindings: false }}
      scopeKey="profile"
      onOrientationSchemaChange={vi.fn()}
      onStepSchemaChange={onStepSchemaChange}
      onDraftValidityChange={onDraftValidityChange}
    />,
  );
  return { onStepSchemaChange, onDraftValidityChange };
}

// The backend half of this contract is schema_dialect.rs, which proves the
// contracts are the published ones and gives the same verdict on each schema.
describe("outputSchemaError agrees with the backend", () => {
  it.each(Object.entries(schemaDialect.contracts))(
    "accepts the published %s contract",
    (_name, schema) => {
      expect(outputSchemaError(schema)).toBeNull();
    },
  );

  it.each(schemaDialect.accepted)("accepts $name", ({ schema }) => {
    expect(outputSchemaError(schema)).toBeNull();
  });

  it.each(schemaDialect.refused)("refuses $name", ({ schema }) => {
    expect(outputSchemaError(schema)).not.toBeNull();
  });
});

describe("SchemaEditorPanel", () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockImplementation((command: string) =>
      command === "preview_provider_schema"
        ? Promise.resolve({
            projected: schemaDialect.reference.projected,
            bytes: 10,
            limit: 100,
            openaiStrict: false,
          })
        : Promise.reject(new Error(command)),
    );
  });

  it("shows a check's result as read-only", () => {
    panel(
      step({
        kind: "analyzer",
        label: "Reference check",
        analyzer: { id: "reference_audit", version: 1 },
      }),
    );
    expect(screen.getByText("Pipeline writes this result")).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(screen.queryByText(/provider/i)).not.toBeInTheDocument();
  });

  it("shows the structure of a findings-v2 reference", async () => {
    panel(step({ output_schema: schemaDialect.reference.schema }));
    expect(await screen.findByText("Structure")).toBeVisible();
    expect(invoke).toHaveBeenCalledWith("preview_provider_schema", {
      schema: schemaDialect.reference.schema,
    });
    expect(screen.getByText("reviewer_ids")).toBeInTheDocument();
  });

  it("lets a contract with a taxonomy be edited and saved", async () => {
    const schema = schemaDialect.contracts.grant_synthesis;
    const { onStepSchemaChange, onDraftValidityChange } = panel(
      step({ output_schema: schema }),
    );
    const editor = await screen.findByRole("textbox", {
      name: "Output JSON schema for Synthesis",
    });
    await userEvent.setup().type(editor, " ");
    await waitFor(() =>
      expect(onStepSchemaChange).toHaveBeenLastCalledWith("synthesis", schema),
    );
    expect(onDraftValidityChange).toHaveBeenLastCalledWith(true);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("keeps the editor and the schema when the text is cleared", async () => {
    const schema = schemaDialect.contracts.grant_synthesis;
    const { onStepSchemaChange, onDraftValidityChange } = panel(
      step({ output_schema: schema }),
    );
    const editor = screen.getByRole("textbox", {
      name: "Output JSON schema for Synthesis",
    });
    await userEvent.setup().clear(editor);
    expect(editor).toBeInTheDocument();
    expect(editor).toHaveValue("");
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Paste a schema or choose Return to Markdown output.",
    );
    expect(onDraftValidityChange).toHaveBeenLastCalledWith(false);
    expect(onStepSchemaChange).not.toHaveBeenCalled();
  });

  it("removes the schema only from the explicit button", async () => {
    const { onStepSchemaChange } = panel(
      step({ output_schema: schemaDialect.contracts.grant_synthesis }),
    );
    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: "Return to Markdown output" }));
    expect(onStepSchemaChange).toHaveBeenLastCalledWith("synthesis", null);
  });

  it("hides the provider view once the draft changes", async () => {
    const user = userEvent.setup();
    panel(step({ output_schema: { type: "object" } }));
    await user.click(
      screen.getByRole("button", { name: "View provider schema" }),
    );
    expect(
      await screen.findByLabelText("Provider schema preview"),
    ).toBeVisible();
    await user.type(
      screen.getByRole("textbox", {
        name: "Output JSON schema for Synthesis",
      }),
      " ",
    );
    expect(
      screen.queryByLabelText("Provider schema preview"),
    ).not.toBeInTheDocument();
  });

  it("does not show a slow preview for one step under another", async () => {
    const user = userEvent.setup();
    let finish: (preview: unknown) => void = () => undefined;
    const schema = { type: "object" };
    const props = {
      scopeKey: "profile",
      onOrientationSchemaChange: vi.fn(),
      onStepSchemaChange: vi.fn(),
      onDraftValidityChange: vi.fn(),
    };
    const view = render(
      <SchemaEditorPanel
        {...props}
        target={{
          kind: "step",
          step: step({ output_schema: schema }),
          publishesFindings: false,
        }}
      />,
    );
    invoke.mockImplementation(
      () => new Promise((resolve) => (finish = resolve)),
    );
    await user.click(
      screen.getByRole("button", { name: "View provider schema" }),
    );
    view.rerender(
      <SchemaEditorPanel
        {...props}
        target={{
          kind: "step",
          step: step({
            id: "other",
            label: "Other",
            output_schema: { type: "object", title: "Other" },
          }),
          publishesFindings: false,
        }}
      />,
    );
    await act(async () =>
      finish({ projected: schema, bytes: 10, limit: 100, openaiStrict: false }),
    );
    expect(
      screen.getByRole("button", { name: "View provider schema" }),
    ).toBeEnabled();
    expect(
      screen.queryByLabelText("Provider schema preview"),
    ).not.toBeInTheDocument();
  });

  it("explains the published findings contract's keywords", async () => {
    panel(step({ output_schema: schemaDialect.reference.schema }));
    await screen.findByText("Structure");
    expect(
      screen.getByText("x-pipeline-findings-taxonomy"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/findings-v1, findings-v2, or findings-v2-validation/),
    ).toBeInTheDocument();
  });
});
