// The docked frame is independent of the lazy file viewers it contains.
import { useLayoutEffect, useRef, type ReactNode } from "react";
import usePersistentPanelWidth from "../hooks/usePersistentPanelWidth";
import IconButton from "../ui/IconButton";
import { Icon } from "../ui/icons";
import ResizeHandle from "./ResizeHandle";

const DEFAULT_WIDTH = 520;
const MIN_WIDTH = 300;
const MAX_WIDTH = 1000;

export default function WorkspaceArtifactsPane({
  id,
  deskWidth,
  onCollapse,
  children,
}: {
  id: string;
  deskWidth: number | null;
  onCollapse: () => void;
  children: ReactNode;
}) {
  const pane = useRef<HTMLElement>(null);
  const [width, setWidth] = usePersistentPanelWidth(
    "pipeline.workspace.artifactsWidth",
    DEFAULT_WIDTH,
    MIN_WIDTH,
    MAX_WIDTH,
  );
  useLayoutEffect(() => {
    const element = pane.current;
    if (!element) return;
    const trigger = document.activeElement;
    element.focus({ preventScroll: true });
    return () => {
      if (
        trigger instanceof HTMLElement &&
        trigger.isConnected &&
        (element.contains(document.activeElement) ||
          document.activeElement === document.body)
      )
        trigger.focus({ preventScroll: true });
    };
  }, []);

  return (
    <section
      ref={pane}
      id={id}
      aria-label="Artifacts"
      tabIndex={-1}
      className="workspace-artifacts-pane"
      onKeyDown={(event) => {
        if (event.key !== "Escape" || event.defaultPrevented) return;
        event.preventDefault();
        event.stopPropagation();
        onCollapse();
      }}
    >
      <ResizeHandle
        label="Resize artifacts"
        edge="left"
        min={MIN_WIDTH}
        max={MAX_WIDTH}
        minRemaining={Math.min(320, (deskWidth ?? 640) / 2)}
        currentWidth={width}
        defaultWidth={DEFAULT_WIDTH}
        onResize={setWidth}
        controlsId={id}
      />
      <header className="workspace-artifacts-header">
        <h2 className="text-sm font-semibold">Artifacts</h2>
        <IconButton label="Collapse artifacts" size="sm" onClick={onCollapse}>
          <Icon name="close" className="h-4 w-4" />
        </IconButton>
      </header>
      <div className="min-h-0 flex-1 overflow-hidden">{children}</div>
    </section>
  );
}
