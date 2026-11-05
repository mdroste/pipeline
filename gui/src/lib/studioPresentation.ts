export function proseForDiff(text: string): string {
  return text
    .replace(/(?<!\\)%[^\n]*/g, "")
    .replace(/\\(?:label|cite\w*|ref)\{[^}]*\}/g, "")
    .replace(/\\(?:textbf|textit|emph|section|subsection)\{([^{}]*)\}/g, "$1")
    .replace(/\s+/g, " ")
    .replace(/([.!?])\s/g, "$1\n")
    .trim();
}
