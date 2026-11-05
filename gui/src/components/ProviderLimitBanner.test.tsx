import { expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import ProviderLimitBanner from "./ProviderLimitBanner";

vi.mock("../lib/platform", () => ({ isMac: true }));

it("drags the macOS window from the banner row without capturing its button", () => {
  render(
    <ProviderLimitBanner
      notices={[
        {
          pass_key: "review",
          label: "Review",
          provider: "claude",
          message: "usage limit",
          status: "exhausted",
          fallback_provider: null,
          fallback_model: null,
        },
      ]}
      onOpenSettings={vi.fn()}
    />,
  );
  // "deep": the row's background drags; Tauri leaves the button's clicks alone.
  expect(screen.getByRole("alert")).toHaveAttribute(
    "data-tauri-drag-region",
    "deep",
  );
  const button = screen.getByRole("button", { name: "Fallback settings" });
  expect(button).not.toHaveAttribute("data-tauri-drag-region");
});
