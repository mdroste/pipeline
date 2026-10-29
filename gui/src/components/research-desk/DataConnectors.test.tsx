import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import DataConnectors from "./DataConnectors";
import { FRED_TERMS_URL } from "./ConnectorNotice";

const mocks = vi.hoisted(() => ({
  fred: vi.fn(),
  series: vi.fn(),
  openUrl: vi.fn(),
}));
vi.mock("../../lib/deskClient", () => ({
  deskClient: { fred: mocks.fred, series: mocks.series },
}));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: mocks.openUrl }));

const dataset = { kind: "dataset", body: {} };
function setup() {
  const onError = vi.fn();
  const run = vi.fn(async (fn: () => Promise<unknown>) => {
    await fn();
  });
  render(
    <DataConnectors workspaceId="w" busy={false} run={run} onError={onError} />,
  );
  return { onError, run };
}
beforeEach(() => {
  mocks.fred.mockReset().mockResolvedValue(dataset);
  mocks.series.mockReset().mockResolvedValue(dataset);
  mocks.openUrl.mockReset().mockResolvedValue(undefined);
});

it("shows the FRED notice and terms link beside the FRED form", () => {
  setup();
  const notice = screen.getByRole("note", { name: "FRED® notice" });
  expect(notice).toHaveTextContent(
    "This product uses the FRED® API but is not endorsed or certified by the Federal Reserve Bank of St. Louis.",
  );
  expect(notice).toHaveTextContent(
    "By using FRED® data, you agree to be bound by the FRED® API Terms of Use.",
  );
  const link = screen.getByRole("link", { name: "FRED® API Terms of Use" });
  expect(link).toHaveAttribute("href", FRED_TERMS_URL);
  fireEvent.click(link);
  expect(mocks.openUrl).toHaveBeenCalledWith(
    "https://fred.stlouisfed.org/docs/api/terms_of_use.html",
  );
  expect(
    screen.getByRole("note", { name: "DBnomics notice" }),
  ).toHaveTextContent("Open Database License");
  expect(
    screen.getByRole("note", { name: "World Bank notice" }),
  ).toHaveTextContent("CC BY 4.0");
});

it("sends a FRED vintage request and clears the key", async () => {
  setup();
  fireEvent.change(screen.getByLabelText("FRED series ID"), {
    target: { value: "GDP" },
  });
  fireEvent.change(screen.getByLabelText("FRED vintage date"), {
    target: { value: "2020-01-01" },
  });
  const key = screen.getByLabelText("FRED API key");
  fireEvent.change(key, { target: { value: "k".repeat(32) } });
  fireEvent.click(
    screen.getByRole("button", { name: "Download this vintage" }),
  );
  await waitFor(() =>
    expect(mocks.fred).toHaveBeenCalledWith(
      "w",
      "GDP",
      "2020-01-01",
      "k".repeat(32),
    ),
  );
  expect(key).toHaveValue("");
});

it("captures DBnomics and World Bank queries and reports failed receipts", async () => {
  const { onError } = setup();
  const dbnomics = screen.getByRole("button", { name: "Download series" });
  expect(dbnomics).toBeDisabled();
  fireEvent.change(screen.getByLabelText("DBnomics series ID"), {
    target: { value: " IMF/WEO:2024-10/USA.NGDP_RPCH.pcent_change " },
  });
  fireEvent.click(dbnomics);
  await waitFor(() =>
    expect(mocks.series).toHaveBeenCalledWith("w", {
      connector: "dbnomics",
      seriesId: "IMF/WEO:2024-10/USA.NGDP_RPCH.pcent_change",
    }),
  );
  mocks.series.mockResolvedValueOnce({
    kind: "acquisition",
    body: { error: "World Bank has no such indicator" },
  });
  fireEvent.change(screen.getByLabelText("World Bank indicator code"), {
    target: { value: "NY.GDP.MKTP.CD" },
  });
  fireEvent.change(screen.getByLabelText("World Bank economies"), {
    target: { value: "USA;BRA" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Download indicator" }));
  await waitFor(() =>
    expect(mocks.series).toHaveBeenLastCalledWith("w", {
      connector: "world_bank",
      indicator: "NY.GDP.MKTP.CD",
      countries: "USA;BRA",
      dateRange: null,
    }),
  );
  await waitFor(() =>
    expect(onError).toHaveBeenCalledWith("World Bank has no such indicator"),
  );
});
