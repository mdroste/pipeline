import type { ReactNode } from "react";
import "./page.css";

/** The one page header, inside a `.ui-page` container: the page's name, one
 * line on what it is for, and at most one main action. The sidebar already
 * shows the product name, so a page header never repeats it. */
export default function PageHeader({
  title,
  description,
  action,
}: {
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <header className="ui-page-header">
      <div>
        <h1>{title}</h1>
        <p className="ui-page-description">{description}</p>
      </div>
      {action}
    </header>
  );
}
