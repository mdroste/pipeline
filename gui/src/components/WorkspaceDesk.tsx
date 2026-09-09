import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";
import useContainerWidth from "../hooks/useContainerWidth";
import usePersistentPanelWidth from "../hooks/usePersistentPanelWidth";
import ResizeHandle from "./ResizeHandle";
import WorkspaceMenu from "./WorkspaceMenu";
import WorkspaceIcon from "./WorkspaceIcon";

type DeskView = "project" | "assistant" | "split";
function loadView(key: string): DeskView {
  try {
    const value = localStorage.getItem(key);
    if (value === "project" || value === "assistant") return value;
  } catch {
    /* Optional device preference. */
  }
  return "split";
}

export default function WorkspaceDesk({
  project,
  children,
  navigationHidden,
  onNavigation,
  hidden,
  assistantOnly,
  assistantRequest = 0,
  projectRequest = 0,
  resetRequest = 0,
  attentionCount = 0,
  active = false,
  onStop,
  navigationButtonRef,
  workspaceId,
  projectName,
  toolLabel,
  onTools,
  toolsDisabled = false,
  navigationOpen = false,
}: {
  project: ReactNode;
  children: ReactNode;
  navigationHidden: boolean;
  onNavigation: () => void;
  hidden?: boolean;
  assistantOnly?: boolean;
  assistantRequest?: number;
  projectRequest?: number;
  resetRequest?: number;
  attentionCount?: number;
  active?: boolean;
  onStop?: () => void;
  workspaceId?: string | null;
  navigationButtonRef?: RefObject<HTMLButtonElement | null>;
  projectName?: string;
  toolLabel?: string;
  onTools?: () => void;
  toolsDisabled?: boolean;
  navigationOpen?: boolean;
}) {
  const [root, width] = useContainerWidth<HTMLDivElement>();
  const preferenceKey = `pipeline.workspace.view.${workspaceId ?? "unfiled"}`;
  const [preferred, setPreferred] = useState<DeskView>(() =>
    loadView(preferenceKey),
  );
  const [assistantWidth, setAssistantWidth] = usePersistentPanelWidth(
    `pipeline.workspace.assistantWidth.${workspaceId ?? "unfiled"}`,
    380,
    360,
    560,
  );
  const [lastPane, setLastPane] = useState<"project" | "assistant">("project");
  // Request counters belong to the page. A newly selected project must not
  // replay another project's old requests over its saved layout preference.
  const requests = useRef({ assistantRequest, projectRequest, resetRequest });
  const projectRef = useRef<HTMLDivElement>(null);
  const assistantRef = useRef<HTMLDivElement>(null);
  const projectButton = useRef<HTMLButtonElement>(null);
  const assistantButton = useRef<HTMLButtonElement>(null);
  const id = useId();
  const fits = width !== null && width >= 848;
  const visibleAssistantWidth = Math.min(assistantWidth, (width ?? 848) - 480);
  const effective =
    !project || assistantOnly
      ? "assistant"
      : preferred === "split" && !fits
        ? lastPane
        : preferred;
  useEffect(() => {
    try {
      localStorage.setItem(preferenceKey, preferred);
    } catch {
      /* Optional preference. */
    }
  }, [preferenceKey, preferred]);
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
      setLastPane("project");
      setPreferred((value) => (value === "assistant" ? "project" : value));
    }
    requests.current.projectRequest = projectRequest;
  }, [projectRequest]);
  const reset = () => {
    setPreferred("split");
    setAssistantWidth(380);
  };
  useEffect(() => {
    if (resetRequest !== requests.current.resetRequest) reset();
    requests.current.resetRequest = resetRequest;
  }, [resetRequest]);
  return (
    <div
      ref={root}
      hidden={hidden}
      className="workspace-desk-area"
      data-workspace-desk
    >
      <div className="workspace-desk-toolbar" aria-label="Desk views">
        <button
          ref={navigationButtonRef}
          type="button"
          onClick={onNavigation}
          aria-label="Browse projects"
          aria-expanded={navigationOpen || !navigationHidden}
          className="workspace-browse-button"
        >
          <WorkspaceIcon name="outline" />
          <span>{projectName ?? "Unfiled conversations"}</span>
        </button>
        {project && onTools && (
          <button
            type="button"
            disabled={toolsDisabled}
            onClick={onTools}
            aria-label="Find a project view"
            title="Go to a project view (⌘/Ctrl K)"
            className="workspace-tool-trigger"
          >
            <span>{toolLabel ?? "Project views"}</span>
            <span aria-hidden="true">⌄</span>
          </button>
        )}
        <span className="flex-1" />
        {project && !assistantOnly && (
          <>
            {effective === "assistant" && (
              <button
                ref={projectButton}
                type="button"
                onClick={() => choose("project")}
              >
                Back to project
              </button>
            )}
            <button
              ref={assistantButton}
              type="button"
              aria-controls={`${id}-assistant`}
              aria-pressed={effective !== "project"}
              aria-label={effective === "project" ? "Show chat" : "Hide chat"}
              onClick={() => {
                if (effective === "project") {
                  setLastPane("assistant");
                  setPreferred(fits ? "split" : "assistant");
                } else choose("project");
              }}
            >
              <WorkspaceIcon name="message" />
              Chat
              {attentionCount > 0
                ? ` · ${attentionCount}`
                : active
                  ? " · Working"
                  : ""}
            </button>
            <WorkspaceMenu label="Project layout">
              <button type="button" onClick={() => choose("project")}>
                Project only
              </button>
              <button type="button" onClick={() => choose("assistant")}>
                Chat only
              </button>
              {fits && (
                <button type="button" onClick={() => setPreferred("split")}>
                  Project + chat
                </button>
              )}
              <button type="button" onClick={reset}>
                Reset layout
              </button>
            </WorkspaceMenu>
            {active && onStop && effective === "project" && (
              <button type="button" onClick={onStop}>
                Stop response
              </button>
            )}
          </>
        )}
        {attentionCount > 0 && (
          <span role="status">
            {attentionCount} request{attentionCount === 1 ? "" : "s"} need
            attention
          </span>
        )}
      </div>
      <div className="workspace-desk-panes" data-layout={effective}>
        {project && (
          <div
            ref={projectRef}
            id={`${id}-project`}
            hidden={effective === "assistant"}
            onFocusCapture={() => setLastPane("project")}
            className="workspace-desk-project"
          >
            {project}
          </div>
        )}
        <div
          ref={assistantRef}
          id={`${id}-assistant`}
          hidden={effective === "project"}
          onFocusCapture={() => setLastPane("assistant")}
          className="workspace-desk-assistant"
          style={
            effective === "split" ? { width: visibleAssistantWidth } : undefined
          }
        >
          {children}
          {effective === "split" && (
            <ResizeHandle
              edge="left"
              label="Resize assistant"
              min={360}
              max={560}
              minRemaining={480}
              currentWidth={visibleAssistantWidth}
              defaultWidth={380}
              onResize={setAssistantWidth}
              controlsId={`${id}-assistant`}
            />
          )}
        </div>
      </div>
    </div>
  );
}
