import { useState, type ComponentProps } from "react";
import ProjectSettings, {
  type ProjectSettingsSection,
} from "./ProjectSettings";

const sections: Array<[ProjectSettingsSection | "tools", string]> = [
  ["publication", "Project"],
  ["documents", "Documents"],
  ["folder", "Folder"],
  ["exclusions", "Exclusions"],
  ["repository", "Git"],
  ["records", "Milestones & checks"],
  ["tools", "More tools"],
];

export default function OverviewOptions(
  props: ComponentProps<typeof ProjectSettings>,
) {
  const [section, setSection] = useState<ProjectSettingsSection | "tools">(
    "publication",
  );
  return (
    <div className="overview-options">
      <div className="overview-sections" aria-label="Project options sections">
        {sections.map(([id, label]) => (
          <button
            type="button"
            key={id}
            aria-pressed={section === id}
            onClick={() => setSection(id)}
          >
            {label}
          </button>
        ))}
      </div>
      <div hidden={section === "tools"}>
        <ProjectSettings
          {...props}
          section={section === "tools" ? "publication" : section}
        />
      </div>
      <div hidden={section !== "tools"} className="overview-tool-grid">
        <button
          type="button"
          className="ui-button"
          onClick={() => props.setTab("execution")}
        >
          Command settings →
        </button>
        <button
          type="button"
          className="ui-button"
          onClick={() => props.setTab("exchange")}
        >
          Share project →
        </button>
        <button
          type="button"
          className="ui-button"
          onClick={() => props.setTab("version-control")}
        >
          Version control →
        </button>
        <button
          type="button"
          className="ui-button"
          onClick={() => props.setTab("review")}
        >
          Send for review →
        </button>
      </div>
      <div className="overview-options-footer">
        <span>Project settings</span>
        <button
          type="button"
          className="project-home-link"
          onClick={() => props.setTab("project-settings")}
        >
          Full project settings →
        </button>
      </div>
    </div>
  );
}
