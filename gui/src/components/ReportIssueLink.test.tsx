import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ReportIssueLink, { ISSUES_URL } from "./ReportIssueLink";

const { openUrl } = vi.hoisted(() => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: openUrl }));

describe("ReportIssueLink", () => {
  beforeEach(() => {
    openUrl.mockReset();
    openUrl.mockResolvedValue(undefined);
  });

  it("opens the issues page in the browser, without the error text", async () => {
    render(
      <p role="alert">
        Could not read /Users/researcher/private/draft.pdf
        <ReportIssueLink />
      </p>,
    );

    const link = screen.getByRole("link", { name: "GitHub Issues" });
    expect(link).toHaveAttribute("href", ISSUES_URL);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "If this looks like a bug, you can report it with this message on GitHub Issues.",
    );

    await userEvent.click(link);

    expect(openUrl).toHaveBeenCalledTimes(1);
    expect(openUrl).toHaveBeenCalledWith(
      "https://github.com/mdroste/pipeline/issues",
    );
  });

  it("gives the address when the browser cannot be opened", async () => {
    openUrl.mockRejectedValue(new Error("no browser"));
    render(<ReportIssueLink />);

    await userEvent.click(screen.getByRole("link", { name: "GitHub Issues" }));

    await waitFor(() =>
      expect(
        screen.getByText(/go to github\.com\/mdroste\/pipeline\/issues/),
      ).toBeInTheDocument(),
    );
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
  });
});
