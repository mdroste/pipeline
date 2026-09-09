export interface FileLocation {
  path: string;
  line?: number;
  page?: number;
  fragment?: string;
}

/** Absolute native paths are opened only after the owning backend validates them. */
export function absoluteLocalFilePath(href: string): string | null {
  if (!href || /[\u0000-\u001f]/.test(href)) return null;
  const [raw] = href.split("#", 1);
  if (raw.includes("?")) return null;
  try {
    const decoded = decodeURIComponent(raw);
    if (decoded.startsWith("/") && !decoded.startsWith("//")) return decoded;
    if (/^[a-z]:[\\/]/i.test(decoded)) return decoded;
  } catch {
    /* Malformed URL escapes are never passed to the native boundary. */
  }
  return null;
}

function absolutePathRelativeToRoot(
  root: string | undefined,
  target: string,
): string | null {
  if (!root) return null;

  const windowsTarget = /^[a-z]:[\\/]/i.test(target);
  const windowsRoot = /^[a-z]:[\\/]/i.test(root);
  const posixTarget = target.startsWith("/") && !target.startsWith("//");
  const posixRoot = root.startsWith("/") && !root.startsWith("//");
  if (!(windowsTarget && windowsRoot) && !(posixTarget && posixRoot))
    return null;

  const normalize = (value: string) =>
    (windowsTarget ? value.replaceAll("\\", "/") : value).replace(/\/$/, "");
  const normalizedRoot = normalize(root);
  const normalizedTarget = normalize(target);
  const comparableRoot = windowsTarget
    ? normalizedRoot.toLowerCase()
    : normalizedRoot;
  const comparableTarget = windowsTarget
    ? normalizedTarget.toLowerCase()
    : normalizedTarget;
  if (!comparableTarget.startsWith(`${comparableRoot}/`)) return null;
  return normalizedTarget.slice(normalizedRoot.length + 1);
}

/** Relative URLs resolve inside their owning project or immutable capture only. */
export function resolveFileLink(
  base: string,
  href: string,
  root?: string,
): FileLocation | null {
  if (!href || href.startsWith("#") || /[\u0000-\u001f]/.test(href))
    return null;
  const [raw, hash = ""] = href.split("#", 2);
  if (raw.includes("?")) return null;
  let relative: string, fragment: string;
  try {
    const decoded = decodeURIComponent(raw);
    relative = absolutePathRelativeToRoot(root, decoded) ?? decoded;
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
