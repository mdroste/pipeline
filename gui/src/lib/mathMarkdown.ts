function fenceMarker(line: string): { marker: string; count: number } | null {
  const trimmed = line.trimStart();
  const marker = trimmed[0];
  if (marker !== "`" && marker !== "~") return null;
  let count = 0;
  while (trimmed[count] === marker) count += 1;
  return count >= 3 ? { marker, count } : null;
}

function normalizeLine(line: string): string {
  let output = "";
  let index = 0;
  let inlineTicks = 0;
  while (index < line.length) {
    if (line[index] === "`") {
      let count = 1;
      while (line[index + count] === "`") count += 1;
      output += line.slice(index, index + count);
      if (inlineTicks === 0) inlineTicks = count;
      else if (inlineTicks === count) inlineTicks = 0;
      index += count;
      continue;
    }
    if (
      inlineTicks === 0 &&
      line[index] === "\\" &&
      line[index - 1] !== "\\" &&
      index + 1 < line.length
    ) {
      const next = line[index + 1];
      if (next === "(" || next === ")") {
        output += "$";
        index += 2;
        continue;
      }
      if (next === "[" || next === "]") {
        output += "$$";
        index += 2;
        continue;
      }
    }
    output += line[index];
    index += 1;
  }
  return output;
}

/** Normalize legacy LaTeX math delimiters without changing code examples. */
export function normalizeMathDelimiters(markdown: string): string {
  let fence: { marker: string; count: number } | null = null;
  return markdown
    .split(/(?<=\n)/)
    .map((line) => {
      const marker = fenceMarker(line);
      if (marker) {
        if (
          fence &&
          marker.marker === fence.marker &&
          marker.count >= fence.count
        ) {
          fence = null;
        } else if (!fence) {
          fence = marker;
        }
        return line;
      }
      return fence ? line : normalizeLine(line);
    })
    .join("");
}
