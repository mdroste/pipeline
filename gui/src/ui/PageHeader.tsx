import type { ReactNode } from "react";
import "./page.css";

/** The one page header, inside a `.ui-page` container: the page's name, one
 * line on what it is for, and at most one main action. The sidebar already
 * shows the product name, so a page header never repeats it. */
export default function PageHeader({
  title,
  description,
  action,
  variant = "standard",
  level = 1,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
  variant?: "standard" | "compact";
  level?: 1 | 2;
}) {
  const Heading = level === 1 ? "h1" : "h2";
  return (
    <header className={`ui-page-header ui-page-header--${variant}`}>
      <div>
        <Heading>{title}</Heading>
        {description && <p className="ui-page-description">{description}</p>}
      </div>
      {action}
    </header>
  );
}
