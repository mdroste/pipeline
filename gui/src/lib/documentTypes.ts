import type { ResearchPaper } from "./workbench/researchTypes";

export const documentTypes = [
  { id: "paper", label: "Paper" },
  { id: "slides", label: "Slides" },
  { id: "appendix", label: "Appendix" },
  { id: "response", label: "Response letter" },
  { id: "other", label: "Other" },
] as const;
export type DocumentType = (typeof documentTypes)[number]["id"];
export function documentType(
  paper: Pick<ResearchPaper, "role" | "documentType">,
): DocumentType {
  return (
    paper.documentType ??
    (paper.role === "manuscript"
      ? "paper"
      : paper.role === "appendix"
        ? "appendix"
        : "other")
  );
}
export const documentTypeLabel = (
  paper: Pick<ResearchPaper, "role" | "documentType">,
) => documentTypes.find((type) => type.id === documentType(paper))!.label;
export const documentRole = (type: DocumentType): ResearchPaper["role"] =>
  type === "paper" ? "manuscript" : type === "appendix" ? "appendix" : "other";
export const documentExtensions = [
  "pdf",
  "tex",
  "md",
  "txt",
  "docx",
  "bib",
  "py",
  "r",
  "jl",
  "do",
  "json",
  "csv",
  "tsv",
];
