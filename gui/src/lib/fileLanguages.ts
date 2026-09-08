import hljs from "highlight.js/lib/common";
import latex from "highlight.js/lib/languages/latex";
import stata from "highlight.js/lib/languages/stata";
import julia from "highlight.js/lib/languages/julia";
import matlab from "highlight.js/lib/languages/matlab";

for (const [name, grammar] of Object.entries({ latex, stata, julia, matlab }))
  hljs.registerLanguage(name, grammar);

export const FILE_LANGUAGES = [
  {
    id: "latex",
    label: "LaTeX / BibTeX",
    extensions: ["tex", "bib", "sty", "cls"],
    aliases: ["tex", "stex", "bibtex"],
  },
  {
    id: "stata",
    label: "Stata",
    extensions: ["do", "ado", "mata"],
    aliases: [],
  },
  { id: "python", label: "Python", extensions: ["py", "pyi"], aliases: ["py"] },
  { id: "r", label: "R", extensions: ["r"], aliases: [] },
  { id: "julia", label: "Julia", extensions: ["jl"], aliases: ["jl"] },
  {
    id: "matlab",
    label: "MATLAB / Octave",
    extensions: ["m"],
    aliases: ["octave"],
  },
  {
    id: "markdown",
    label: "Markdown",
    extensions: ["md", "markdown", "rmd", "qmd"],
    aliases: ["md"],
  },
  { id: "sql", label: "SQL", extensions: ["sql"], aliases: [] },
  {
    id: "bash",
    label: "Shell",
    extensions: ["sh", "bash", "zsh"],
    aliases: ["sh", "shell", "zsh"],
  },
  {
    id: "json",
    label: "JSON",
    extensions: ["json", "jsonl", "ipynb"],
    aliases: [],
  },
  { id: "yaml", label: "YAML", extensions: ["yaml", "yml"], aliases: ["yml"] },
  {
    id: "ini",
    label: "TOML / INI",
    extensions: ["toml", "ini", "cfg"],
    aliases: ["toml"],
  },
  {
    id: "javascript",
    label: "JavaScript",
    extensions: ["js", "jsx", "mjs", "cjs"],
    aliases: ["js", "jsx"],
  },
  {
    id: "typescript",
    label: "TypeScript",
    extensions: ["ts", "tsx"],
    aliases: ["ts", "tsx"],
  },
  { id: "rust", label: "Rust", extensions: ["rs"], aliases: ["rs"] },
  { id: "c", label: "C", extensions: ["c", "h"], aliases: [] },
  {
    id: "cpp",
    label: "C++",
    extensions: ["cpp", "cc", "hpp"],
    aliases: ["c++"],
  },
  { id: "java", label: "Java", extensions: ["java"], aliases: [] },
  { id: "go", label: "Go", extensions: ["go"], aliases: [] },
  { id: "ruby", label: "Ruby", extensions: ["rb"], aliases: [] },
  { id: "css", label: "CSS", extensions: ["css", "scss"], aliases: [] },
  {
    id: "xml",
    label: "HTML / XML",
    extensions: ["html", "xml", "svg"],
    aliases: ["html"],
  },
  {
    id: "plaintext",
    label: "Plain text",
    extensions: ["txt", "log", "csv", "tsv"],
    aliases: ["text", "plain"],
  },
] as const;

export function fileLanguage(path: string): string {
  const extension = path.split(".").pop()?.toLowerCase();
  return (
    FILE_LANGUAGES.find((l) =>
      (l.extensions as readonly string[]).includes(extension ?? ""),
    )?.id ?? "plaintext"
  );
}
export function languageId(name: string): string {
  const lower = name.toLowerCase();
  return (
    FILE_LANGUAGES.find(
      (l) => l.id === lower || (l.aliases as readonly string[]).includes(lower),
    )?.id ?? (hljs.getLanguage(lower) ? lower : "plaintext")
  );
}
export function highlightedSource(
  text: string,
  language: string,
): string | null {
  if (text.length > 200_000 || language === "plaintext") return null;
  try {
    return hljs.highlight(text, {
      language: languageId(language),
      ignoreIllegals: true,
    }).value;
  } catch {
    return null;
  }
}

export function texCompletions(
  texts: string[],
): Array<{ label: string; type: string }> {
  const values = new Map<string, string>();
  for (const text of texts) {
    for (const match of text.matchAll(/\\label\{([^}]+)\}/g))
      values.set(match[1], "constant");
    for (const match of text.matchAll(/@\w+\s*\{\s*([^,\s]+)\s*,/g))
      values.set(match[1], "keyword");
  }
  return [...values].map(([label, type]) => ({ label, type }));
}
