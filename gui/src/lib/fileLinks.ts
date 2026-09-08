export interface FileLocation {
  path: string;
  line?: number;
  page?: number;
  fragment?: string;
}

/** Relative URLs resolve inside their owning project or immutable capture only. */
export function resolveFileLink(
  base: string,
  href: string,
): FileLocation | null {
  if (
    !href ||
    href.startsWith("#") ||
    /^[a-z][\w+.-]*:/i.test(href) ||
    /^[\/\\]/.test(href) ||
    /[\u0000-\u001f\\]/.test(href)
  )
    return null;
  const [raw, hash = ""] = href.split("#", 2);
  if (raw.includes("?")) return null;
  let relative: string, fragment: string;
  try {
    relative = decodeURIComponent(raw);
    fragment = decodeURIComponent(hash);
  } catch {
    return null;
  }
  if (
    /^[\/\\]/.test(relative) ||
    /[\u0000-\u001f\\]/.test(relative) ||
    /^[a-z][\w+.-]*:/i.test(relative)
  )
    return null;
  const parts = base.split("/").slice(0, -1);
  for (const part of relative.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") {
      if (!parts.length) return null;
      parts.pop();
    } else parts.push(part);
  }
  if (
    !parts.length ||
    parts.some(
      (p) =>
        p === ".git" ||
        p === ".pipeline-tasks" ||
        p.startsWith(".pipeline-apply-"),
    )
  )
    return null;
  const line = fragment.match(/^L(\d+)(?:-L?\d+)?$/i);
  const page = fragment.match(/^page=(\d+)$/i);
  return {
    path: parts.join("/"),
    ...(line
      ? { line: Math.max(1, Number(line[1])) }
      : page
        ? { page: Math.max(1, Number(page[1])) }
        : fragment
          ? { fragment }
          : {}),
  };
}
