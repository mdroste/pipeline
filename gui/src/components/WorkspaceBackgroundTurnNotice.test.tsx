import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import WorkspaceBackgroundTurnNotice from "./WorkspaceBackgroundTurnNotice";

it("links a running automation to Automations, where it can be stopped", () => {
  const onOpen = vi.fn();
  const onOpenAutomations = vi.fn();
  render(
    <WorkspaceBackgroundTurnNotice
      title="Tariff paper"
      waiting={0}
      onOpen={onOpen}
      onOpenAutomations={onOpenAutomations}
    />,
  );
  expect(
    screen.getByText(
      "An automation is running in “Tariff paper”. You can send a message here when it finishes.",
    ),
  ).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Open Automations" }));
  expect(onOpenAutomations).toHaveBeenCalledOnce();
  expect(onOpen).not.toHaveBeenCalled();
});

it("sends an automation's approval to its conversation, where it can be answered", () => {
  const onOpen = vi.fn();
  render(
    <WorkspaceBackgroundTurnNotice
      title="Tariff paper"
      waiting={1}
      onOpen={onOpen}
      onOpenAutomations={vi.fn()}
    />,
  );
  expect(
    screen.getByText(
      "A response in “Tariff paper” is waiting for your approval.",
    ),
  ).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Open conversation" }));
  expect(onOpen).toHaveBeenCalledOnce();
});
