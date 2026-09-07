import type { CSSProperties, ReactNode } from "react";

type IconName = "copy" | "check" | "plus" | "outline" | "research" | "export" | "archive" | "arrow-up" | "arrow-down" | "message";
const paths: Record<IconName, ReactNode> = {
  copy: <><rect x="8" y="8" width="12" height="12" rx="3" /><path d="M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3" /></>,
  check: <path d="m5 12 4 4L19 6" />,
  plus: <path d="M12 5v14M5 12h14" />,
  outline: <><path d="M9 5h11M9 12h11M9 19h11" /><path d="M4 5h.01M4 12h.01M4 19h.01" /></>,
  research: <><path d="M10 3h4M11 3v6l-6 9a2 2 0 0 0 1.7 3h10.6a2 2 0 0 0 1.7-3l-6-9V3M8 15h8" /></>,
  export: <><path d="M12 15V3m-4 4 4-4 4 4M5 13v6a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-6" /></>,
  archive: <><rect x="3" y="3" width="18" height="4" rx="1" /><path d="M5 7v13h14V7M10 11h4" /></>,
  "arrow-up": <path d="M12 19V5m-6 6 6-6 6 6" />,
  "arrow-down": <path d="M12 5v14m-6-6 6 6 6-6" />,
  message: <path d="M20 11.5a8.5 8.5 0 0 1-12 7.7L3 21l1.8-5A8.5 8.5 0 1 1 20 11.5Z" />,
};
export default function WorkspaceIcon({ name, size = 16, style }: { name: IconName; size?: number; style?: CSSProperties }) {
  return <svg aria-hidden="true" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.65" strokeLinecap="round" strokeLinejoin="round" style={{ flexShrink: 0, ...style }}>{paths[name]}</svg>;
}
