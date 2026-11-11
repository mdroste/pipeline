import { act, render } from "@testing-library/react";
import { expect, it } from "vitest";
import { useAppAppearance } from "./useAppAppearance";
import { saveAppPreferences, readAppPreferences } from "../lib/appPreferences";
import ReportViewer from "../components/ReportViewer";

function Preview() {
  useAppAppearance();
  return <ReportViewer markdown="# Report\n\nText" />;
}
it("scales the interface at 100, 110, and 125 percent while keeping report and editor preferences independent", () => {
  const previous = readAppPreferences();
  act(() =>
    saveAppPreferences({
      interfaceScale: 100,
      reportScale: 110,
      editorFontSize: 18,
    }),
  );
  const { container, unmount } = render(<Preview />);
  for (const scale of [100, 110, 125]) {
    act(() => saveAppPreferences({ interfaceScale: scale }));
    expect(document.documentElement.style.fontSize).toBe(
      `${(16 * scale) / 100}px`,
    );
    const report = container.querySelector<HTMLElement>(".report-content")!;
    expect(report.style.fontSize).toBe("17.6px");
    expect(report.style.getPropertyValue("--report-unit")).toBe("1.1px");
    expect(readAppPreferences().editorFontSize).toBe(18);
  }
  unmount();
  act(() => saveAppPreferences(previous));
  document.documentElement.style.fontSize = "";
});
