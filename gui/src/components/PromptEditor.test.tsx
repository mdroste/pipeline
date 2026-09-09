import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import PromptEditor from "./PromptEditor";

describe("PromptEditor", () => {
  it("shows placeholder chips for contexts that have them", () => {
    render(
      <PromptEditor
        value=""
        onChange={() => {}}
        context={{ kind: "merge" }}
        ariaLabel="Merge prompt"
      />,
    );
    expect(
      screen.getByRole("textbox", { name: "Merge prompt" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Insert:")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "{topic}" })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "{agent_reports}" }),
    ).toBeInTheDocument();
  });

  it("hides the chip row for parallel step prompts (no placeholders)", () => {
    render(
      <PromptEditor
        value=""
        onChange={() => {}}
        context={{ kind: "parallel" }}
        ariaLabel="Parallel prompt"
      />,
    );
    expect(screen.queryByText("Insert:")).not.toBeInTheDocument();
  });

  it("inserts a token at the cursor when a chip is clicked", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <PromptEditor
        value="Merge these."
        onChange={onChange}
        context={{ kind: "merge" }}
        ariaLabel="Merge prompt"
      />,
    );
    // Without focusing the textarea, selectionStart is 0 — token is prepended.
    await user.click(screen.getByRole("button", { name: "{topic}" }));
    expect(onChange).toHaveBeenCalledWith("{topic}Merge these.");
  });

  it("lints unknown placeholders with line numbers", () => {
    render(
      <PromptEditor
        value={"fine\n{unknown_thing}"}
        onChange={() => {}}
        context={{ kind: "merge" }}
        ariaLabel="Merge prompt"
      />,
    );
    expect(screen.getByText("1 unknown placeholder:")).toBeInTheDocument();
    expect(
      screen.getByText(/\{unknown_thing\} \(line 2\)/),
    ).toBeInTheDocument();
  });

  it("shows no lint warning for valid placeholders", () => {
    render(
      <PromptEditor
        value="{topic} and {agent_reports}"
        onChange={() => {}}
        context={{ kind: "merge" }}
        ariaLabel="Merge prompt"
      />,
    );
    expect(screen.queryByText(/unknown placeholder/)).not.toBeInTheDocument();
  });
});
