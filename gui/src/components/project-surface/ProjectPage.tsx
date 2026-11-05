import type { ReactNode } from "react";
import {
  destinationHints,
  type WorkspaceDestination,
} from "../../lib/workspaceNavigation";
import "./ProjectPage.css";

/** A project view's scrolling page. The subnav names the view, so the page
 * opens with a line on what it is for instead of repeating the title. */
export default function ProjectPage({
  view,
  children,
}: {
  view: WorkspaceDestination;
  children: ReactNode;
}) {
  const hint = destinationHints[view];
  return (
    <div className="project-page">
      <div className="project-page-body">
        {hint && <p className="project-page-hint">{hint}</p>}
        {children}
      </div>
    </div>
  );
}
