import { describe, it, expect, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  invoke,
  makeConfig,
  mockLoad,
  useAdvancedEditor,
  resetPipelinePageMocks,
} from "./PipelinePage.test-support";
import PipelinePage from "./PipelinePage";

describe("PipelinePage schemas", () => {
  beforeEach(resetPipelinePageMocks);

  it("applies stock orientation prompts and schemas as one contract", async () => {
    const config = makeConfig();
    const schema = {
      type: "object",
      required: ["overview"],
      properties: { overview: { type: "string" } },
    };
    mockLoad(config, {
      get_orientation_defaults: {
        prompt: "Build the generic survey. {input_text}",
        schema,
      },
    });
    const user = userEvent.setup();
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("button", { name: "Identify content" }));
    await user.click(
      screen.getByRole("button", { name: "Insert generic survey" }),
    );
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const saveCall = invoke.mock.calls.find(
        (call) => call[0] === "save_pipeline_config",
      );
      expect(saveCall?.[1].config).toMatchObject({
        orientation_prompt: "Build the generic survey. {input_text}",
        orientation_schema: schema,
      });
    });
  });

  it("opens on the workflow overview and swaps it for the selected step", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    expect(screen.getByText("Review overview")).toBeInTheDocument();
    expect(screen.queryByText("Select a step to edit")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Extract" })).toBeInTheDocument();

    // Selecting a stage from the overview opens its editor in place.
    await user.click(screen.getAllByText("Technical")[0]);
    expect(await screen.findByLabelText("Step label")).toHaveValue("Technical");
    expect(screen.queryByText("Review overview")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Back to overview" }));
    expect(screen.getByText("Review overview")).toBeInTheDocument();
  });

  it("keeps effective artifact schemas visible in a dedicated low-clutter view", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");
    await useAdvancedEditor(user);

    await user.click(screen.getByRole("button", { name: "Schemas" }));
    expect(
      screen.getByRole("navigation", { name: "Schema stages" }),
    ).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Document overview schema" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Technical output schema" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Consolidate Issues output schema" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Document overview schema" }),
    ).toBeInTheDocument();
    expect(
      screen.getByLabelText("Effective document overview schema"),
    ).toHaveTextContent('"type": "object"');

    await user.click(
      screen.getByRole("button", { name: "Technical output schema" }),
    );
    expect(
      screen.getByRole("heading", { name: "Technical output schema" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        /stores only its content field as the step’s Markdown report/i,
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByLabelText("Effective schema for Technical"),
    ).toHaveTextContent('"content"');
    expect(
      screen.queryByRole("textbox", {
        name: "Output JSON schema for Technical",
      }),
    ).not.toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: "Create custom JSON schema" }),
    );
    const editor = screen.getByRole("textbox", {
      name: "Output JSON schema for Technical",
    });
    expect((editor as HTMLTextAreaElement).value).toContain('"result"');
    fireEvent.change(editor, {
      target: {
        value:
          '{"type":"object","required":["score"],"properties":{"score":{"type":"number"}}}',
      },
    });
    expect(
      screen.getByText("Valid portable Pipeline artifact schema."),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      const saveCall = invoke.mock.calls.find(
        (call) => call[0] === "save_pipeline_config",
      );
      expect(saveCall?.[1].config.steps[0].output_schema).toEqual({
        type: "object",
        required: ["score"],
        properties: { score: { type: "number" } },
      });
    });
  });

  it("keeps invalid schema drafts local and moves schema editing out of execution rules", async () => {
    const user = userEvent.setup();
    const config = makeConfig();
    config.steps[0].output_schema = {
      type: "object",
      required: ["result"],
      properties: { result: { type: "string" } },
    };
    mockLoad(config);
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");
    await useAdvancedEditor(user);

    await user.click(screen.getByRole("button", { name: "Schemas" }));
    await user.click(
      screen.getByRole("button", { name: "Technical output schema" }),
    );
    const editor = screen.getByRole("textbox", {
      name: "Output JSON schema for Technical",
    });
    fireEvent.change(editor, {
      target: {
        value:
          '{"type":"object","required":["score"],"properties":{"score":{"type":"number"}}}',
      },
    });
    expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
    fireEvent.change(editor, {
      target: { value: '{"type":"object","additionalProperties":false}' },
    });
    expect(screen.getByRole("alert")).toHaveTextContent("unsupported keyword");
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    fireEvent.change(editor, {
      target: { value: '{"type":"array","items":{"type":"string"}}' },
    });
    expect(screen.getByRole("alert")).toHaveTextContent(
      "root must explicitly be 'object'",
    );
    fireEvent.change(editor, { target: { value: '{"type":' } });
    expect(screen.getByRole("alert")).toHaveTextContent("Invalid schema");
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Edit stage" }));
    await user.click(screen.getByRole("tab", { name: "When it runs" }));
    expect(
      screen.queryByRole("textbox", { name: "Output JSON schema" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText(/Define result formats in Schemas/),
    ).toBeInTheDocument();
  });

  it("shows and saves the undone schema after Undo in the schema editor", async () => {
    const user = userEvent.setup();
    const config = makeConfig();
    config.steps[0].output_schema = {
      type: "object",
      required: ["result"],
      properties: { result: { type: "string" } },
    };
    mockLoad(config);
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");
    await useAdvancedEditor(user);
    await user.click(screen.getByRole("button", { name: "Schemas" }));
    await user.click(
      screen.getByRole("button", { name: "Technical output schema" }),
    );
    const editor = screen.getByRole("textbox", {
      name: "Output JSON schema for Technical",
    }) as HTMLTextAreaElement;
    fireEvent.change(editor, {
      target: {
        value:
          '{"type":"object","required":["score"],"properties":{"score":{"type":"number"}}}',
      },
    });
    expect(editor.value).toContain('"score"');

    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(editor.value).toContain('"result"');
    expect(editor.value).not.toContain('"score"');

    await user.click(screen.getByRole("button", { name: "Redo" }));
    expect(editor.value).toContain('"score"');
    await user.click(screen.getByRole("button", { name: "Undo" }));
    // Undo leaves nothing to save; make an unrelated edit to enable Save.
    await user.click(screen.getByRole("button", { name: "Edit stage" }));
    await user.type(screen.getByLabelText("Step label"), " proofs");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      const saveCall = invoke.mock.calls.find(
        (call) => call[0] === "save_pipeline_config",
      );
      expect(saveCall?.[1].config.steps[0].output_schema).toEqual({
        type: "object",
        required: ["result"],
        properties: { result: { type: "string" } },
      });
    });
  });

  it("keeps the last valid schema structure while the draft is invalid", async () => {
    const user = userEvent.setup();
    const config = makeConfig();
    config.steps[0].output_schema = {
      type: "object",
      required: ["result"],
      properties: { result: { type: "string" } },
    };
    mockLoad(config);
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");
    await useAdvancedEditor(user);
    await user.click(screen.getByRole("button", { name: "Schemas" }));
    await user.click(
      screen.getByRole("button", { name: "Technical output schema" }),
    );
    expect(screen.getByLabelText("Schema structure")).toHaveTextContent(
      "result",
    );
    fireEvent.change(
      screen.getByRole("textbox", { name: "Output JSON schema for Technical" }),
      { target: { value: '{"type":"object","properties":{"result":' } },
    );
    expect(screen.getByLabelText("Schema structure")).toHaveTextContent(
      "result",
    );
    expect(
      screen.getByText("Showing the last valid version"),
    ).toBeInTheDocument();
  });
});
