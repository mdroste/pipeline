import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AboutPage from "./AboutPage";
import { version as appVersion } from "../../package.json";
import {
  FLAPPY_HIGH_SCORE_KEY,
  persistFlappyHighScore,
  readFlappyHighScore,
} from "../lib/flappyScore";

const getVersion = vi.hoisted(() => vi.fn());
const openUrl = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/app", () => ({ getVersion }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: openUrl }));

describe("AboutPage", () => {
  beforeEach(() => {
    localStorage.clear();
    getVersion.mockReset();
    getVersion.mockResolvedValue(appVersion);
    openUrl.mockReset();
  });

  it("shows the current application version", async () => {
    render(<AboutPage onClose={() => {}} />);
    expect(
      await screen.findByText(`Pipeline v${appVersion}`),
    ).toBeInTheDocument();
  });

  it("uses the standard page header, with Help as its only title", async () => {
    const close = vi.fn();
    const { container } = render(<AboutPage onClose={close} />);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      /^Help$/,
    );
    expect(container.querySelector(".ui-page .ui-page-header")).toBeTruthy();
    expect(container.querySelector(".ui-page-description")).toHaveTextContent(
      /what each part of the app does/,
    );
    expect(
      screen.getByRole("heading", { level: 2, name: "Set up" }),
    ).toBeVisible();
    await userEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(close).toHaveBeenCalledOnce();
    await screen.findByText(`Pipeline v${appVersion}`);
  });

  it("opens the game after the full Konami code and requires a pilot choice", async () => {
    localStorage.setItem(FLAPPY_HIGH_SCORE_KEY, "12");
    const context = {
      arc: vi.fn(),
      beginPath: vi.fn(),
      bezierCurveTo: vi.fn(),
      clearRect: vi.fn(),
      closePath: vi.fn(),
      createLinearGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
      ellipse: vi.fn(),
      fill: vi.fn(),
      fillRect: vi.fn(),
      fillText: vi.fn(),
      lineTo: vi.fn(),
      moveTo: vi.fn(),
      quadraticCurveTo: vi.fn(),
      restore: vi.fn(),
      rotate: vi.fn(),
      save: vi.fn(),
      scale: vi.fn(),
      setTransform: vi.fn(),
      stroke: vi.fn(),
      strokeRect: vi.fn(),
      translate: vi.fn(),
    };
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(
      () => context as unknown as CanvasRenderingContext2D,
    );
    render(<AboutPage onClose={() => undefined} />);

    const keys = [
      "ArrowUp",
      "ArrowUp",
      "ArrowDown",
      "ArrowDown",
      "ArrowLeft",
      "ArrowRight",
      "ArrowLeft",
      "ArrowRight",
      "B",
      "A",
    ];
    keys.forEach((key) => fireEvent.keyDown(document, { key }));
    expect(
      screen.queryByRole("dialog", { name: "Flappy Pipeline" }),
    ).not.toBeInTheDocument();

    fireEvent.keyDown(document, { key: "Enter" });
    const dialog = await screen.findByRole("dialog", {
      name: "Flappy Pipeline",
    });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog).toHaveTextContent(/Best\s*12/i);

    const dario = screen.getByRole("button", { name: "Choose DARIO" });
    const sam = screen.getByRole("button", { name: "Choose SAM" });
    expect(dario).toBeInTheDocument();
    expect(screen.queryByText("Dario Amodei")).not.toBeInTheDocument();
    expect(screen.queryByText("Sam Altman")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Start flight" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByLabelText(/Flappy Pipeline game board/),
    ).not.toBeInTheDocument();

    await userEvent.setup().click(sam);
    expect(
      screen.getByLabelText(/Flappy Pipeline game board.*Ready to play/i),
    ).toBeInTheDocument();
    expect(screen.getByText("SAM")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Start flight" }),
    ).toBeInTheDocument();

    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: "Change character" }));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Choose DARIO" }),
      ).toHaveFocus(),
    );

    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: "Close Flappy Pipeline" }));
    expect(
      screen.queryByRole("dialog", { name: "Flappy Pipeline" }),
    ).not.toBeInTheDocument();
  });

  it("keeps the larger high score in local storage", () => {
    expect(readFlappyHighScore()).toBe(0);
    expect(persistFlappyHighScore(7, 3)).toBe(7);
    expect(readFlappyHighScore()).toBe(7);
    expect(persistFlappyHighScore(4, 7)).toBe(7);
    expect(readFlappyHighScore()).toBe(7);
  });

  it("explains the product areas, built-in reviews, and provider data sharing", () => {
    render(<AboutPage onClose={() => undefined} />);

    expect(
      screen.getByText(/Chat with an AI\s+assistant, keep each paper/i),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/A conversation is a chat with the AI assistant/i),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Automations" }),
    ).toBeInTheDocument();
    expect(screen.getByText("~/.pipeline/workbench/")).toBeInTheDocument();
    expect(
      screen.getByText(/don't depend on each other run at the same time/i),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/sends its prompt and the material it can read/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/plan and data-use terms/i)).toBeInTheDocument();
    expect(screen.getByText("Reviews")).toBeInTheDocument();
    expect(screen.getByText("Paper Review")).toBeInTheDocument();
    expect(screen.getByText("Paper Review (Fast)")).toBeInTheDocument();
    expect(screen.getByText("Slide Review")).toBeInTheDocument();
    expect(screen.getByText("Grant Proposal Review")).toBeInTheDocument();
    expect(screen.getByText("Declaude")).toBeInTheDocument();
    expect(screen.queryByText("Codebase Review")).not.toBeInTheDocument();
    expect(
      screen.queryByText("Replication Package Audit"),
    ).not.toBeInTheDocument();
    // Once in setup (highly recommended) and once under How Pipeline reads documents.
    expect(screen.getAllByText("PaddleOCR-VL Full Parser")).toHaveLength(2);
    expect(screen.getByText("Highly recommended")).toBeInTheDocument();
    expect(screen.queryByText("PaddleOCR-VL Fast")).not.toBeInTheDocument();
    // Researchers see "review design"; workflow and profile are internal names.
    expect(screen.queryByText(/workflow/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/profile/i)).not.toBeInTheDocument();
  });

  it("describes each built-in review by what it does", () => {
    render(<AboutPage onClose={() => undefined} />);

    expect(
      screen.getByText(/checks them against the paper/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/Finds formulaic prose/i)).toBeInTheDocument();
    expect(
      screen.queryByText(/configured review step/i),
    ).not.toBeInTheDocument();
  });

  it("says conversations use the chosen provider", () => {
    render(<AboutPage onClose={() => undefined} initialSection="privacy" />);

    expect(
      screen.getByText(/to the provider you\s+chose/i),
    ).toBeInTheDocument();
    expect(screen.queryByText(/managed ChatGPT/i)).not.toBeInTheDocument();
  });

  it("lists each report tab with a plain description", () => {
    render(<AboutPage onClose={() => undefined} />);

    expect(screen.getByText("Report details")).toBeInTheDocument();
    expect(
      screen.getByText(/Findings you can accept, reject, or annotate/i),
    ).toBeInTheDocument();
  });

  it("keeps reference sections collapsed by default and opens privacy on request", () => {
    const { unmount } = render(<AboutPage onClose={() => undefined} />);
    const collapsed = screen.getByText("Data & privacy").closest("details");
    expect(collapsed).not.toBeNull();
    expect(collapsed).not.toHaveAttribute("open");
    expect(screen.getByText("Reviews").closest("details")).not.toHaveAttribute(
      "open",
    );
    unmount();

    render(<AboutPage onClose={() => undefined} initialSection="privacy" />);
    expect(
      screen.getByText("Data & privacy").closest("details"),
    ).toHaveAttribute("open");
    expect(screen.getByText("Reviews").closest("details")).not.toHaveAttribute(
      "open",
    );
  });

  it("navigates to the described page from section links", () => {
    const onNavigate = vi.fn();
    render(<AboutPage onClose={() => undefined} onNavigate={onNavigate} />);

    fireEvent.click(screen.getByRole("button", { name: "Open Settings" }));
    fireEvent.click(screen.getByRole("button", { name: "Open Automations" }));
    fireEvent.click(
      screen.getByRole("button", { name: "Open Review designer" }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Open Gallery" }));
    fireEvent.click(
      screen.getByRole("button", { name: "Open Review history" }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Open review collections" }),
    );

    expect(onNavigate.mock.calls.map((call) => call[0])).toEqual([
      "settings",
      "tasks",
      "pipeline",
      "gallery",
      "history",
      "projects",
    ]);
  });

  it("hides navigation links when no navigator is provided", () => {
    render(<AboutPage onClose={() => undefined} />);
    expect(
      screen.queryByRole("button", { name: "Open Review designer" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Open Settings" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Install in Settings" }),
    ).not.toBeInTheDocument();
  });

  it("links the recommended parser to the Settings install card", () => {
    const onOpenPdfSettings = vi.fn();
    render(
      <AboutPage
        onClose={() => undefined}
        onOpenPdfSettings={onOpenPdfSettings}
      />,
    );

    fireEvent.click(
      screen.getByRole("button", { name: "Install in Settings" }),
    );
    expect(onOpenPdfSettings).toHaveBeenCalledTimes(1);
  });

  it("surfaces a shell failure for the privacy details link", async () => {
    openUrl.mockRejectedValueOnce(new Error("no browser is configured"));
    render(<AboutPage onClose={() => undefined} initialSection="privacy" />);

    await userEvent
      .setup()
      .click(screen.getByRole("link", { name: "Full privacy details" }));

    expect(openUrl).toHaveBeenCalledWith(
      "https://github.com/mdroste/pipeline/blob/main/PRIVACY.md",
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      /could not open the link.*no browser is configured/i,
    );
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
