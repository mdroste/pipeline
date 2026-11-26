import { Icon } from "../ui/icons";
import IconButton from "../ui/IconButton";
import { reportExportClient } from "../lib/reportExportClient";
import { notify } from "./DialogService";

export default function PdfExportResult({
  path,
  onDismiss,
}: {
  path: string;
  onDismiss: () => void;
}) {
  return (
    <div className="ui-overlay-frame px-3 py-2 text-left text-ui-label">
      <div className="flex items-start justify-between gap-3">
        <p className="font-medium">PDF saved</p>
        <IconButton
          type="button"
          label="Dismiss PDF export result"
          onClick={onDismiss}
        >
          <Icon name="close" className="h-4 w-4" />
        </IconButton>
      </div>
      <p className="mt-1 break-all" title={path}>
        {path}
      </p>
      <button
        onClick={() => {
          void reportExportClient
            .reveal({ path })
            .catch((error: unknown) =>
              notify(`Could not show the PDF: ${String(error)}`),
            );
        }}
        className="ui-button ui-button--sm mt-2"
      >
        Show in folder
      </button>
    </div>
  );
}
