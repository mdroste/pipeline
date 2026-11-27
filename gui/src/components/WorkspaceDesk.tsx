// Project and conversation panes for one project. The conversation docks to
// the left, right or bottom and may take any room the project can spare. When
// the desk is too small for both, one pane shows at a time and both stay
// mounted, so drafts survive; growing restores the saved choice. View, dock
// side and sizes are per-project device preferences and grant no access. When
// artifacts open alongside the chat, the project pane stays mounted for the
// return to split.

import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import { WorkspacePaneVisibility } from "./workspacePaneVisibility";
import ProjectLocation from "./ProjectLocation";
import "../ui/page.css";
import useContainerWidth from "../hooks/useContainerWidth";
import usePersistentPanelWidth from "../hooks/usePersistentPanelWidth";
import ResizeHandle from "./ResizeHandle";
import WorkspaceArtifactsPane from "./WorkspaceArtifactsPane";
import WorkspaceMenu from "./WorkspaceMenu";
import WorkspaceIcon from "./WorkspaceIcon";
import { MenuItem, MenuLabel, MenuSeparator } from "../ui/Menu";
import {
  assistantDock,
  DeskDockContext,
  type AssistantDock,
  type DeskDock,
} from "./deskDock";
import { Icon } from "../ui/icons";
import { isMac } from "../lib/platform";
import "./WorkspaceDesk.css";

type DeskView = "project" | "assistant" | "split";
function loadView(key: string): DeskView | null {
  try {
    const value = localStorage.getItem(key);
    if (value === "project" || value === "assistant" || value === "split")
      return value;
  } catch {
    /* Optional device preference. */
  }
  return null;
}

function subscribeVisibility(listener: () => void) {
  document.addEventListener("visibilitychange", listener);
  return () => document.removeEventListener("visibilitychange", listener);
}
const windowVisible = () => document.visibilityState !== "hidden";

/** The conversation pane's default and smallest size, in pixels. */
const ASSISTANT = { width: 440, height: 320, minWidth: 320, minHeight: 200 };
/** The project keeps at least this much room; the conversation may take the rest. */
const PROJECT_MIN = { width: 400, height: 180 };
/** Sizes are bounded by the desk, not by a fixed maximum. */
const UNBOUNDED = 10_000;

function loadDock(key: string): AssistantDock {
  try {
    return assistantDock(localStorage.getItem(key)) ?? "right";
  } catch {
    return "right";
  }
}

/**
 * The side a pane dragged to (x, y) would land on: the nearest of the left,
 * right and bottom edges, or none from the middle of the desk.
 */
export function dockZone(
  box: { left: number; top: number; width: number; height: number },
  x: number,
  y: number,
): AssistantDock | null {
  if (box.width <= 0 || box.height <= 0) return null;
  const fx = (x - box.left) / box.width;
  const fy = (y - box.top) / box.height;
  if (fx < 0 || fx > 1 || fy < 0 || fy > 1) return null;
  const distances: [AssistantDock, number][] = [
    ["left", fx],
    ["right", 1 - fx],
    ["bottom", 1 - fy],
  ];
  const [side, distance] = distances.reduce((a, b) => (b[1] < a[1] ? b : a));
  return distance <= 0.35 ? side : null;
}

export default function WorkspaceDesk({
  project,
  artifacts,
  children,
  hidden,
  assistantOnly,
  assistantRequest = 0,
  projectRequest = 0,
  resetRequest = 0,
  conversationFirstRequest = 0,
  attentionCount = 0,
  active = false,
  onStop,
  stopping = false,
  feedback,
  assistantFeedback,
  workspaceId,
  projectName,
  toolLabel,
  onTools,
  toolsDisabled = false,
}: {
  project: ReactNode;
  artifacts?: { id: string; onCollapse: () => void; content: ReactNode };
  children: ReactNode;
  hidden?: boolean;
  assistantOnly?: boolean;
  assistantRequest?: number;
  projectRequest?: number;
  resetRequest?: number;
  /** Bumped for a project with nothing but conversations: show the chat alone
      unless the researcher already chose a layout for this project. */
  conversationFirstRequest?: number;
  attentionCount?: number;
  active?: boolean;
  onStop?: () => void;
  stopping?: boolean;
  feedback?: ReactNode;
  /** The conversation's own messages, shown here only while its pane is hidden. */
  assistantFeedback?: ReactNode;
  workspaceId?: string | null;
  projectName?: string;
  toolLabel?: string;
  onTools?: () => void;
  toolsDisabled?: boolean;
}) {
  const visible = useSyncExternalStore(subscribeVisibility, windowVisible);
  const [root, width] = useContainerWidth<HTMLDivElement>();
  const preferenceKey = `pipeline.workspace.view.${workspaceId ?? "unfiled"}`;
  const [preferred, setPreferred] = useState<DeskView>(
    () => loadView(preferenceKey) ?? "split",
  );
  // A layout counts as chosen once one was saved for this project or the
  // researcher changed it here. Only an unchosen layout follows the
  // conversation-first default, and that default is never saved, so a project
  // that later gains a paper opens on the split view again.
  const [hadPreference] = useState(() => loadView(preferenceKey) !== null);
  const chosen = useRef(hadPreference);
  const automatic = useRef(false);
  const [assistantWidth, setAssistantWidth] = usePersistentPanelWidth(
    `pipeline.workspace.assistantWidth.${workspaceId ?? "unfiled"}`,
    ASSISTANT.width,
    ASSISTANT.minWidth,
    UNBOUNDED,
  );
  const [assistantHeight, setAssistantHeight] = usePersistentPanelWidth(
    `pipeline.workspace.assistantHeight.${workspaceId ?? "unfiled"}`,
    ASSISTANT.height,
    ASSISTANT.minHeight,
    UNBOUNDED,
  );
  const dockKey = `pipeline.workspace.assistantDock.${workspaceId ?? "unfiled"}`;
  const [dock, setDockState] = useState<AssistantDock>(() => loadDock(dockKey));
  const setDock = (next: AssistantDock) => {
    setDockState(next);
    try {
      localStorage.setItem(dockKey, next);
    } catch {
      /* Optional preference. */
    }
  };
  const bottom = dock === "bottom";
  // The panes' height decides whether a bottom dock fits; the desk's width
  // decides for the sides.
  const panes = useRef<HTMLDivElement>(null);
  const [height, setHeight] = useState<number | null>(null);
  useLayoutEffect(() => {
    const element = panes.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver((entries) => {
      const value = entries[0]?.contentRect.height;
      if (value && value > 0) setHeight(value);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const [dockHint, setDockHint] = useState<AssistantDock | null>(null);
  const dockHintRef = useRef<AssistantDock | null>(null);
  const [lastPane, setLastPane] = useState<"project" | "assistant">("project");
  // Request counters belong to the page. A newly selected project must not
  // replay another project's old requests over its saved layout preference.
  const requests = useRef({
    assistantRequest,
    projectRequest,
    resetRequest,
    conversationFirstRequest,
  });
  const projectRef = useRef<HTMLDivElement>(null);
  const assistantRef = useRef<HTMLDivElement>(null);
  const projectButton = useRef<HTMLButtonElement>(null);
  const assistantButton = useRef<HTMLButtonElement>(null);
  const id = useId();
  const fits = bottom
    ? height === null || height >= ASSISTANT.minHeight + PROJECT_MIN.height
    : width !== null && width >= ASSISTANT.minWidth + PROJECT_MIN.width;
  const visibleAssistantSize = bottom
    ? height === null
      ? assistantHeight
      : Math.min(assistantHeight, height - PROJECT_MIN.height)
    : Math.min(assistantWidth, (width ?? UNBOUNDED) - PROJECT_MIN.width);
  const effective =
    !project || assistantOnly || (artifacts && preferred === "split")
      ? "assistant"
      : preferred === "split" && !fits
        ? lastPane
        : preferred;
  useEffect(() => {
    if (automatic.current) {
      automatic.current = false;
      return;
    }
    if (!chosen.current && preferred === "split") return;
    chosen.current = true;
    try {
      localStorage.setItem(preferenceKey, preferred);
    } catch {
      /* Optional preference. */
    }
  }, [preferenceKey, preferred]);
  useEffect(() => {
    if (
      conversationFirstRequest !== requests.current.conversationFirstRequest &&
      !chosen.current &&
      preferred === "split"
    ) {
      automatic.current = true;
      setLastPane("assistant");
      setPreferred("assistant");
    }
    requests.current.conversationFirstRequest = conversationFirstRequest;
  }, [conversationFirstRequest]);
  useLayoutEffect(() => {
    if (
      effective === "project" &&
      assistantRef.current?.contains(document.activeElement)
    )
      assistantButton.current?.focus();
    if (
      effective === "assistant" &&
      projectRef.current?.contains(document.activeElement)
    )
      assistantButton.current?.focus();
  }, [effective]);
  const choose = (pane: "project" | "assistant") => {
    artifacts?.onCollapse();
    setLastPane(pane);
    setPreferred(pane);
  };
  useEffect(() => {
    if (assistantRequest !== requests.current.assistantRequest) {
      setLastPane("assistant");
      setPreferred((value) => (value === "project" ? "assistant" : value));
    }
    requests.current.assistantRequest = assistantRequest;
  }, [assistantRequest]);
  useEffect(() => {
    if (projectRequest !== requests.current.projectRequest) {
      artifacts?.onCollapse();
      setLastPane("project");
      setPreferred((value) => (value === "assistant" ? "project" : value));
    }
    requests.current.projectRequest = projectRequest;
  }, [projectRequest]);
  const split = () => {
    artifacts?.onCollapse();
    // Choosing the default explicitly still counts as a choice.
    chosen.current = true;
    try {
      localStorage.setItem(preferenceKey, "split");
    } catch {
      /* Optional preference. */
    }
    setPreferred("split");
  };
  const reset = () => {
    split();
    setDock("right");
    setAssistantWidth(ASSISTANT.width);
    setAssistantHeight(ASSISTANT.height);
  };
  /** Moves the conversation to a side, beside the project when there is room. */
  const dockTo = (side: AssistantDock) => {
    setDock(side);
    const room = panes.current?.getBoundingClientRect();
    const sideFits =
      side === "bottom"
        ? !room?.height ||
          room.height >= ASSISTANT.minHeight + PROJECT_MIN.height
        : width !== null && width >= ASSISTANT.minWidth + PROJECT_MIN.width;
    if (sideFits) split();
  };
  const deskDock: DeskDock | null =
    project && !assistantOnly
      ? {
          dock,
          move: (x, y) => {
            const box = panes.current?.getBoundingClientRect();
            const next = box ? dockZone(box, x, y) : null;
            dockHintRef.current = next;
            setDockHint((old) => (old === next ? old : next));
          },
          end: (drop) => {
            const target = dockHintRef.current;
            dockHintRef.current = null;
            setDockHint(null);
            if (drop && target) dockTo(target);
          },
        }
      : null;
  useEffect(() => {
    if (resetRequest !== requests.current.resetRequest) reset();
    requests.current.resetRequest = resetRequest;
  }, [resetRequest]);
  return (
    <div
      ref={root}
      hidden={hidden}
      className="workspace-desk-area ui-layout-workspace"
      data-workspace-desk
    >
      {/* On macOS the toolbar's background also drags the window ("deep":
          its buttons still take their own clicks). */}
      <div
        className="workspace-desk-toolbar"
        aria-label="Desk views"
        data-tauri-drag-region={isMac ? "deep" : undefined}
      >
        {workspaceId && projectName ? (
          <ProjectLocation
            projectId={workspaceId}
            name={projectName}
            disabled={toolsDisabled}
          />
        ) : (
          <span className="workspace-desk-title">Unfiled conversations</span>
        )}
        {project && onTools && (
          <button
            type="button"
            disabled={toolsDisabled}
            onClick={onTools}
            aria-label="Find a project view"
            title="Go to a project view"
            className="workspace-desk-button workspace-tool-trigger"
          >
            <span>{toolLabel ?? "Project views"}</span>
            <Icon name="chevron-down" className="h-3.5 w-3.5 shrink-0" />
          </button>
        )}
        <span className="flex-1" />
        {project && !assistantOnly && (
          <>
            {effective === "assistant" && (
              <button
                ref={projectButton}
                type="button"
                className="workspace-desk-button"
                onClick={() => choose("project")}
              >
                Back to project
              </button>
            )}
            <button
              ref={assistantButton}
              type="button"
              className="workspace-desk-button"
              aria-controls={`${id}-assistant`}
              aria-pressed={effective !== "project"}
              aria-label={
                effective === "project"
                  ? "Show conversation"
                  : "Hide conversation"
              }
              onClick={() => {
                if (effective === "project") {
                  setLastPane("assistant");
                  setPreferred(fits ? "split" : "assistant");
                } else choose("project");
              }}
            >
              <WorkspaceIcon name="message" />
              Conversation
              {attentionCount > 0
                ? ` · ${attentionCount}`
                : active
                  ? " · Working"
                  : ""}
            </button>
            <WorkspaceMenu label="Project layout">
              <MenuItem
                checked={effective === "project"}
                onSelect={() => choose("project")}
              >
                Project only
              </MenuItem>
              <MenuItem
                checked={effective === "assistant"}
                onSelect={() => choose("assistant")}
              >
                Conversation only
              </MenuItem>
              {fits && (
                <MenuItem checked={effective === "split"} onSelect={split}>
                  Project + conversation
                </MenuItem>
              )}
              <MenuSeparator />
              <MenuLabel>Conversation position</MenuLabel>
              {(["left", "right", "bottom"] as const).map((side) => (
                <MenuItem
                  key={side}
                  checked={dock === side}
                  onSelect={() => dockTo(side)}
                >
                  {side === "left"
                    ? "Left"
                    : side === "right"
                      ? "Right"
                      : "Bottom"}
                </MenuItem>
              ))}
              <MenuSeparator />
              <MenuItem onSelect={reset}>Reset layout</MenuItem>
            </WorkspaceMenu>
            {active && onStop && effective === "project" && (
              <button
                type="button"
                className="workspace-desk-button"
                onClick={onStop}
                disabled={stopping}
              >
                {stopping ? "Stopping…" : "Stop response"}
              </button>
            )}
          </>
        )}
        {attentionCount > 0 && (
          <span role="status">
            {attentionCount === 1
              ? "1 request needs attention"
              : `${attentionCount} requests need attention`}
          </span>
        )}
      </div>
      {feedback}
      {effective === "project" && assistantFeedback}
      <div
        ref={panes}
        className="workspace-desk-panes"
        data-layout={effective}
        data-dock={dock}
      >
        {project && (
          <div
            ref={projectRef}
            id={`${id}-project`}
            hidden={effective === "assistant"}
            onFocusCapture={() => setLastPane("project")}
            className="workspace-desk-project"
          >
            <WorkspacePaneVisibility.Provider
              value={visible && !hidden && effective !== "assistant"}
            >
              {project}
            </WorkspacePaneVisibility.Provider>
          </div>
        )}
        <div
          ref={assistantRef}
          id={`${id}-assistant`}
          hidden={effective === "project"}
          onFocusCapture={() => setLastPane("assistant")}
          className="workspace-desk-assistant"
          style={
            effective === "split"
              ? { [bottom ? "height" : "width"]: visibleAssistantSize }
              : undefined
          }
        >
          <WorkspacePaneVisibility.Provider
            value={visible && !hidden && effective !== "project"}
          >
            <DeskDockContext.Provider value={deskDock}>
              {children}
            </DeskDockContext.Provider>
          </WorkspacePaneVisibility.Provider>
          {effective === "split" && (
            <ResizeHandle
              key={dock}
              edge={dock === "left" ? "right" : bottom ? "top" : "left"}
              label="Resize conversation"
              min={bottom ? ASSISTANT.minHeight : ASSISTANT.minWidth}
              max={UNBOUNDED}
              minRemaining={bottom ? PROJECT_MIN.height : PROJECT_MIN.width}
              currentWidth={visibleAssistantSize}
              defaultWidth={bottom ? ASSISTANT.height : ASSISTANT.width}
              onResize={bottom ? setAssistantHeight : setAssistantWidth}
              controlsId={`${id}-assistant`}
            />
          )}
        </div>
        {dockHint && dockHint !== dock && (
          <div className="workspace-dock-hint" data-zone={dockHint} />
        )}
        {effective !== "project" && artifacts && (
          <WorkspaceArtifactsPane
            id={artifacts.id}
            deskWidth={width}
            onCollapse={artifacts.onCollapse}
          >
            {artifacts.content}
          </WorkspaceArtifactsPane>
        )}
      </div>
    </div>
  );
}
