import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AboutPage from "./AboutPage";

const getVersion = vi.hoisted(() => vi.fn());
const openUrl = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/app", () => ({ getVersion }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: openUrl }));

describe("AboutPage", () => {
  beforeEach(() => {
    getVersion.mockReset();
    getVersion.mockResolvedValue("1.0.1");
    openUrl.mockReset();
  });

  it("describes dependency scheduling and scoped artifact access", () => {
    render(<AboutPage onClose={() => undefined} />);

    expect(screen.getByText(/declared dependencies are ready/i)).toBeInTheDocument();
    expect(screen.getByText(/upstream artifacts selected/i)).toBeInTheDocument();
    expect(screen.getByText(/provider account, plan, and usage terms/i)).toBeInTheDocument();
  });

  it.each([
    ["github.com/mdroste/pipeline", "https://github.com/mdroste/pipeline"],
    [
      "third-party licenses",
      "https://github.com/mdroste/pipeline/blob/main/THIRD_PARTY_LICENSES.md",
    ],
  ])("surfaces a shell failure for the %s link", async (name, url) => {
    openUrl.mockRejectedValueOnce(new Error("no browser is configured"));
    render(<AboutPage onClose={() => undefined} />);

    await userEvent.setup().click(screen.getByRole("link", { name }));

    expect(openUrl).toHaveBeenCalledWith(url);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      /could not open the link.*no browser is configured/i,
    );
  });
});
