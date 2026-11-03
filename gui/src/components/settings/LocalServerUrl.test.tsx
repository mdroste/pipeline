import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { expect, it, vi } from "vitest";
import { makeSettings } from "../../test/settingsFixtures";
import type { Settings } from "../../lib/types";
import LocalServerUrl from "./LocalServerUrl";

const STORED = "__PIPELINE_STORED_SECRET__";
const original = "http://localhost:11434/v1";

function Harness({
  initial,
  onChange,
}: {
  initial: Settings;
  onChange: (settings: Settings) => void;
}) {
  const [settings, setSettings] = useState(initial);
  return (
    <LocalServerUrl
      settings={settings}
      setSettings={(next) => {
        setSettings(next);
        onChange(next);
      }}
    />
  );
}
const setup = (local_api_key: string) => {
  const onChange = vi.fn();
  render(
    <Harness
      initial={{
        ...makeSettings(),
        local_base_url: original,
        local_api_key,
      }}
      onChange={onChange}
    />,
  );
  return {
    onChange,
    user: userEvent.setup(),
    field: screen.getByLabelText("Local Server URL"),
  };
};

it("does not touch a saved key while the address is being typed", async () => {
  const { user, field, onChange } = setup(STORED);
  await user.type(field, "x");
  expect(onChange).not.toHaveBeenCalled();
  await user.keyboard("{Backspace}");
  await user.tab();
  expect(onChange).not.toHaveBeenCalled();
  expect(screen.queryByRole("alert")).toBeNull();
});

it("asks before a new address removes a saved key", async () => {
  const { user, field, onChange } = setup(STORED);
  await user.clear(field);
  await user.type(field, "http://lab.example/v1");
  await user.tab();
  expect(onChange).not.toHaveBeenCalled();
  expect(screen.getByRole("alert")).toHaveTextContent(
    "changing the address removes it",
  );
  await user.click(
    screen.getByRole("button", { name: "Change server and remove key" }),
  );
  expect(onChange).toHaveBeenCalledTimes(1);
  expect(onChange.mock.calls[0][0]).toMatchObject({
    local_base_url: "http://lab.example/v1",
    local_api_key: "",
  });
  expect(screen.queryByRole("alert")).toBeNull();
});

it("keeps the key and the server when the change is declined", async () => {
  const { user, field, onChange } = setup(STORED);
  await user.type(field, "x{Enter}");
  await user.click(screen.getByRole("button", { name: "Keep current server" }));
  expect(onChange).not.toHaveBeenCalled();
  expect(field).toHaveValue(original);
  expect(screen.queryByRole("alert")).toBeNull();
});

it("treats a key saved on this visit the same as one saved earlier", async () => {
  const { user, field, onChange } = setup("typed-key");
  await act(async () => {
    await user.type(field, "x");
    await user.tab();
  });
  expect(onChange).not.toHaveBeenCalled();
  expect(screen.getByRole("alert")).toBeVisible();
});

it("saves a new address when editing ends if there is no key to lose", async () => {
  const { user, field, onChange } = setup("");
  await user.clear(field);
  await user.type(field, "http://lab.example/v1");
  expect(onChange).not.toHaveBeenCalled();
  await user.tab();
  expect(onChange.mock.calls[0][0]).toMatchObject({
    local_base_url: "http://lab.example/v1",
    local_api_key: "",
  });
});
