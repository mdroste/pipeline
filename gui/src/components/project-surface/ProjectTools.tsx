// Browse the same named routes as search and deep links, without mounting any tools.
import { useState } from "react";
import {
  allProjectViews,
  directoryGroup,
  directoryGroups,
  destinationHints,
  workspaceDestinations,
  sectionLabel,
  type WorkspaceDestination,
} from "../../lib/workspaceNavigation";
import { Icon } from "../../ui/icons";

export default function ProjectTools({
  onSelect,
}: {
  onSelect: (id: WorkspaceDestination) => void;
}) {
  const [query, setQuery] = useState("");
  const terms = query.toLocaleLowerCase().trim().split(/\s+/);
  const matches = allProjectViews.filter(
    (id) =>
      id !== "project-tools" &&
      terms.every((term) =>
        [
          id,
          workspaceDestinations[id].label,
          destinationHints[id],
          sectionLabel(id),
          directoryGroup(id),
        ]
          .join(" ")
          .toLocaleLowerCase()
          .includes(term),
      ),
  );
  return (
    <section className="focused-page" aria-label="Project tools">
      <header>
        <div>
          <h1>Project tools</h1>
          <p>Browse all the views in this project.</p>
        </div>
      </header>
      <input
        type="search"
        className="focused-search"
        aria-label="Search project tools"
        placeholder="Find a view or tool…"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      <div className="focused-directory">
        {directoryGroups.map((group) => {
          const views = matches.filter((id) => directoryGroup(id) === group);
          return (
            views.length > 0 && (
              <section key={group}>
                <h2>{group}</h2>
                {views.map((id) => (
                  <button
                    className="focused-tool"
                    type="button"
                    key={id}
                    onClick={() => onSelect(id)}
                  >
                    <span>
                      <strong>{workspaceDestinations[id].label}</strong>
                      <small>{destinationHints[id]}</small>
                    </span>
                    <Icon name="chevron-right" />
                  </button>
                ))}
              </section>
            )
          );
        })}
      </div>
      {!matches.length && <p role="status">No matching views.</p>}
    </section>
  );
}
