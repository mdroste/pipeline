import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import postcss from "postcss";
import { COLOR_THEMES } from "../lib/theme";

// Test the resolved token contract independently of UI rendering. Palette rules
// have greater specificity than App.css defaults, so they are applied last.
const styles = ["../App.css", "./color-themes.css"].map((path) =>
  postcss.parse(readFileSync(new URL(path, import.meta.url), "utf8")),
);
type RGB = [number, number, number];
function tokens(theme: string, dark: boolean) {
  const element = document.documentElement;
  element.dataset.colorTheme = theme;
  element.classList.toggle("dark", dark);
  const values = new Map<string, string>();
  for (const css of styles)
    css.walkRules((rule) => {
      if (
        !rule.nodes.some(
          (node) => node.type === "decl" && node.prop.startsWith("--"),
        )
      )
        return;
      if (rule.parent?.type === "atrule" || !element.matches(rule.selector))
        return;
      rule.walkDecls((decl) => {
        if (decl.prop.startsWith("--")) values.set(decl.prop, decl.value);
      });
    });
  const resolve = (value: string): string =>
    value.replace(/var\((--[\w-]+)\)/g, (_, key: string) => {
      const found = values.get(key);
      if (!found) throw new Error(`Missing token ${key}`);
      return resolve(found);
    });
  return (name: string, behind: RGB = [255, 255, 255]): RGB => {
    const value = resolve(values.get(name)!);
    if (value.startsWith("#"))
      return [1, 3, 5].map((i) => parseInt(value.slice(i, i + 2), 16)) as RGB;
    const channels = value.match(/[\d.]+/g)!.map(Number);
    const alpha = channels[3] ?? 1;
    return channels
      .slice(0, 3)
      .map((channel, i) => channel * alpha + behind[i] * (1 - alpha)) as RGB;
  };
}
function luminance(rgb: RGB) {
  const linear = rgb.map((value) => {
    const n = value / 255;
    return n <= 0.04045 ? n / 12.92 : ((n + 0.055) / 1.055) ** 2.4;
  });
  return linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722;
}
function contrast(a: RGB, b: RGB) {
  const [low, high] = [luminance(a), luminance(b)].sort((x, y) => x - y);
  return (high + 0.05) / (low + 0.05);
}
describe.each(
  COLOR_THEMES.flatMap(({ id }) => [
    { id, dark: false },
    { id, dark: true },
  ]),
)("$id / dark=$dark semantic contrast", ({ id, dark }) => {
  it("keeps action, selection, and status text readable", () => {
    const color = tokens(id, dark);
    for (const surface of ["--ui-surface", "--ui-raised"]) {
      const background = color(surface);
      for (const text of ["--ui-text", "--ui-muted"])
        expect(
          contrast(color(text), background),
          `${text} on ${surface}`,
        ).toBeGreaterThanOrEqual(4.5);
      expect(
        contrast(color("--ui-selection-indicator"), background),
        "selection indicator",
      ).toBeGreaterThanOrEqual(3);
      for (const status of ["danger", "warning", "success", "running"]) {
        const foreground = color(`--ui-${status}`);
        expect(
          contrast(foreground, background),
          `${status} on ${surface}`,
        ).toBeGreaterThanOrEqual(4.5);
        expect(
          contrast(foreground, color(`--ui-${status}-soft`, background)),
          `${status} on soft`,
        ).toBeGreaterThanOrEqual(4.5);
      }
    }
    expect(
      contrast(color("--ui-on-action"), color("--ui-action")),
      "primary action",
    ).toBeGreaterThanOrEqual(4.5);
    expect(
      contrast(color("--ui-selection"), color("--ui-selection-soft")),
      "selection text",
    ).toBeGreaterThanOrEqual(4.5);
  });
});
