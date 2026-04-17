import { save, open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import type { PipelineReport } from "../lib/types";

interface Props {
  markdown: string;
  report?: PipelineReport;
  extractedText?: string;
}

export default function ExportControls({ markdown, report, extractedText }: Props) {
  const handleSaveMd = async () => {
    const path = await save({
      defaultPath: `PIPELINE_REPORT_${new Date().toISOString().slice(0, 10)}.md`,
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    if (path) {
      try {
        await invoke("save_report_md", { path, markdown });
      } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : String(e);
        alert(`Failed to save: ${msg}`);
      }
    }
  };

  const handlePrint = async () => {
    try {
      await invoke("print_report_html", { markdown });
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      alert(`Failed to generate PDF: ${msg}`);
    }
  };

  const handleSaveAll = async () => {
    if (!report || !extractedText) return;
    const dir = await open({
      directory: true,
      multiple: false,
      title: "Choose folder for artifacts",
    });
    if (dir) {
      try {
        await invoke("save_all_artifacts", {
          dir,
          markdown,
          extractedText,
          report,
        });
        alert("All artifacts saved.");
      } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : String(e);
        alert(`Failed to save artifacts: ${msg}`);
      }
    }
  };

  return (
    <div className="flex gap-2">
      <button
        onClick={handleSaveMd}
        className="py-1.5 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                   text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
      >
        Save MD
      </button>
      <button
        onClick={handlePrint}
        className="py-1.5 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                   text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
      >
        Save PDF
      </button>
      {report && extractedText && (
        <button
          onClick={handleSaveAll}
          className="py-1.5 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                     text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
        >
          Save All
        </button>
      )}
    </div>
  );
}
