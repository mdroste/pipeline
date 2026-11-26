import { Icon } from "../../ui/icons";
import IconButton from "../../ui/IconButton";
import { useState } from "react";
import type {
  OverviewAction,
  OverviewBlock,
  OverviewRow,
} from "../../lib/project-overview/types";

export function OverviewRowView({
  row,
  busy,
  onAction,
}: {
  row: OverviewRow;
  busy?: boolean;
  onAction: (action: OverviewAction) => void;
}) {
  return (
    <div
      className="project-overview-row"
      data-attention={row.attention || undefined}
    >
      <div>
        <p title={row.text}>{row.text}</p>
        {row.detail && (
          <p className="project-home-meta" title={row.detail}>
            {row.detail}
          </p>
        )}
      </div>
      <button
        type="button"
        className="project-home-link"
        data-project-tour={
          row.action.kind === "run" ? "open-report" : undefined
        }
        disabled={busy && row.action.kind === "fetch"}
        onClick={() => onAction(row.action)}
      >
        {busy && row.action.kind === "fetch"
          ? "Checking…"
          : `${row.action.label} →`}
      </button>
    </div>
  );
}

/** Every derived row stays reachable without making the dashboard grow. */
export default function OverviewRows({
  block,
  busy,
  onAction,
  pageSize = 3,
  preview = false,
}: {
  block: OverviewBlock;
  busy?: boolean;
  onAction: (action: OverviewAction) => void;
  pageSize?: number;
  /** The card's Details action exposes the complete paginated block. */
  preview?: boolean;
}) {
  const [index, setIndex] = useState(0);
  const pages = Math.max(1, Math.ceil(block.rows.length / pageSize));
  const page = preview ? 0 : Math.min(index, pages - 1);
  return (
    <section className="overview-rows" aria-label={block.title}>
      <div aria-live="polite">
        {block.rows.slice(page * pageSize, (page + 1) * pageSize).map((row) => (
          <OverviewRowView
            key={row.id}
            row={row}
            busy={busy}
            onAction={onAction}
          />
        ))}
      </div>
      {!preview && pages > 1 && (
        <nav
          className="overview-pagination"
          aria-label={`${block.title} pages`}
        >
          <span>
            {page * pageSize + 1}–
            {Math.min((page + 1) * pageSize, block.rows.length)} of{" "}
            {block.rows.length}
          </span>
          <IconButton
            type="button"

            label={`Previous ${block.title.toLowerCase()}`}
            disabled={page === 0}
            onClick={() => setIndex(page - 1)}
          >
            <Icon name="arrow-left" className="h-4 w-4" />
          </IconButton>
          <IconButton
            type="button"

            label={`Next ${block.title.toLowerCase()}`}
            disabled={page + 1 === pages}
            onClick={() => setIndex(page + 1)}
          >
            <Icon name="arrow" className="h-4 w-4" />
          </IconButton>
        </nav>
      )}
    </section>
  );
}
