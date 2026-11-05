import { useState, type ReactNode } from "react";
import { Icon } from "./icons";
import "./fold.css";

/** A titled section that starts closed, for setup forms and rarely used tools.
 * Its contents stay mounted, so a half-filled form survives closing it. */
export default function Fold({
  title,
  open = false,
  children,
}: {
  title: string;
  /** Whether it starts open. Later changes leave the researcher's choice. */
  open?: boolean;
  children: ReactNode;
}) {
  const [initial] = useState(open);
  return (
    <details className="ui-fold" open={initial}>
      <summary>
        <Icon name="chevron-right" className="h-3.5 w-3.5" />
        {title}
      </summary>
      <div className="ui-fold-body">{children}</div>
    </details>
  );
}
