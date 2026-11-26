import type { CSSProperties } from "react";
import { Icon, type IconName } from "../ui/icons";

/** Compatibility names; shared semantic icons own every path. */
const names = {
  copy: "copy",
  check: "check",
  plus: "plus",
  outline: "notes",
  research: "review",
  export: "export",
  archive: "archive",
  "arrow-up": "arrow-up",
  "arrow-down": "arrow-down",
  message: "assistant",
  settings: "settings",
  close: "close",
  file: "file",
} as const satisfies Record<string, IconName>;

export default function WorkspaceIcon({
  name,
  size = 16,
  style,
}: {
  name: keyof typeof names;
  size?: number;
  style?: CSSProperties;
}) {
  return (
    <Icon
      name={names[name]}
      className="shrink-0"
      style={{ width: size, height: size, ...style }}
    />
  );
}
