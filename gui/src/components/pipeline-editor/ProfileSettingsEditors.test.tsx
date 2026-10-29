import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { InputSlot, VarSpec } from "../../lib/types";
import {
  MemoizedExtraInputsEditor as ExtraInputsEditor,
  MemoizedVariablesEditor as VariablesEditor,
} from "./ProfileSettingsEditors";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

function Harness({
  onVariables,
  onSlots,
}: {
  onVariables: (variables: VarSpec[]) => void;
  onSlots: (slots: InputSlot[]) => void;
}) {
  const [variables, setVariables] = useState<VarSpec[]>([
    { key: "tone", label: "Tone", kind: "choice", default: "" },
  ]);
  const [slots, setSlots] = useState<InputSlot[]>([
    { key: "rubric", label: "Rubric", mode: "document" },
  ]);
  return (
    <>
      <VariablesEditor
        variables={variables}
        onChange={(next) => {
          setVariables(next);
          onVariables(next);
        }}
      />
      <ExtraInputsEditor
        slots={slots}
        onChange={(next) => {
          setSlots(next);
          onSlots(next);
        }}
      />
    </>
  );
}

describe("comma-separated profile lists", () => {
  it("keeps commas and spaces while typing and commits the parsed list", async () => {
    const user = userEvent.setup();
    const onVariables = vi.fn();
    const onSlots = vi.fn();
    render(<Harness onVariables={onVariables} onSlots={onSlots} />);

    const choices = screen.getByRole("textbox", {
      name: "Variable Tone choices",
    });
    await user.type(choices, "Minor revision, Major revision, ");
    expect(choices).toHaveValue("Minor revision, Major revision, ");
    expect(onVariables).not.toHaveBeenCalled();
    await user.tab();
    expect(onVariables).toHaveBeenLastCalledWith([
      expect.objectContaining({
        choices: ["Minor revision", "Major revision"],
      }),
    ]);
    expect(choices).toHaveValue("Minor revision, Major revision");

    const extensions = screen.getByRole("textbox", {
      name: "Allowed extensions for Rubric",
    });
    await user.type(extensions, ".PDF, docx{Enter}");
    expect(onSlots).toHaveBeenLastCalledWith([
      expect.objectContaining({ extensions: ["pdf", "docx"] }),
    ]);
    expect(extensions).toHaveValue("pdf, docx");
  });
});
