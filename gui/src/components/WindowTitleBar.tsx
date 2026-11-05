import type { Ref } from "react";
import IconButton from "../ui/IconButton";
import { Icon } from "../ui/icons";

/** A stable title strip above both the sidebar and page content. */
export default function WindowTitleBar({
  isMac,
  isWindows = false,
  collapsed,
  onToggleSidebar,
  toggleRef,
}: {
  isMac: boolean;
  isWindows?: boolean;
  collapsed: boolean;
  onToggleSidebar: () => void;
  toggleRef?: Ref<HTMLButtonElement>;
}) {
  return (
    <div
      data-testid="window-title-bar"
      data-tauri-drag-region={isMac || isWindows ? "deep" : undefined}
      className={`relative shrink-0 select-none border-b border-gray-200/80 bg-gray-100/90 dark:border-neutral-800 dark:bg-[#101010] ${isMac ? "h-7" : "h-8"}`}
    >
      <IconButton
        ref={toggleRef}
        label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        aria-expanded={!collapsed}
        tooltipSide="bottom"
        size="sm"
        className={`absolute !h-6 !w-6 ${isMac ? "left-[76px] top-1 z-40" : "left-3 top-0.5 z-10"}`}
        onClick={onToggleSidebar}
      >
        <Icon name="sidebar" />
      </IconButton>
    </div>
  );
}
