export interface FileLocation {
  path: string;
  line?: number;
  page?: number;
  fragment?: string;
}

/** Normalize only filesystem namespaces we can scope to an authorized root. */
function nativeAbsolute(
  value: string,
): { path: string; windows: boolean; unc: boolean } | null {
  if (/[\u0000-\u001f]/.test(value)) return null;
  if (/^\\\\\?\\UNC\\/i.test(value)) value = `\\\\${value.slice(8)}`;
  else if (/^\\\\\?\\[a-z]:\\/i.test(value)) value = value.slice(4);
  if (value.includes("?")) return null;
  if (/^[a-z]:[\\/]/i.test(value))
    return { path: value.replaceAll("\\", "/"), windows: true, unc: false };
  // Backslash UNC paths are native paths; //host URLs stay external URLs.
  const unc = /^\\\\([^\\/:?]+)\\([^\\/:?]+)(?:\\|$)/.exec(value);
  if (unc && ![".", ".."].includes(unc[1]) && ![".", ".."].includes(unc[2]))
    return { path: value.replaceAll("\\", "/"), windows: true, unc: true };
  if (value.startsWith("/") && !value.startsWith("//"))
    return { path: value, windows: false, unc: false };
  return null;
}

/** Absolute native paths are opened only after the owning backend validates them. */
export function isNativeFileLink(href: string): boolean {
  try {
    return nativeAbsolute(decodeURIComponent(href.split("#", 1)[0])) !== null;
  } catch {
    return false;
  }
}

export function absoluteLocalFilePath(href: string): string | null {
  if (!href || /[\u0000-\u001f]/.test(href)) return null;
  const [raw] = href.split("#", 1);
  try {
    const absolute = nativeAbsolute(decodeURIComponent(raw));
    // UNC is supported only when mapped to an explicit matching scope below.
    if (absolute && !absolute.unc) return absolute.path;
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

  const absoluteRoot = nativeAbsolute(root);
  const absoluteTarget = nativeAbsolute(target);
  if (
    !absoluteRoot ||
    !absoluteTarget ||
    absoluteRoot.windows !== absoluteTarget.windows ||
    absoluteRoot.unc !== absoluteTarget.unc
  )
    return null;
  const normalizedRoot = absoluteRoot.path.replace(/\/$/, "");
  const normalizedTarget = absoluteTarget.path.replace(/\/$/, "");
  const comparableRoot = absoluteTarget.windows
    ? normalizedRoot.toLowerCase()
    : normalizedRoot;
  const comparableTarget = absoluteTarget.windows
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
  let fromRoot = false;
  let relative: string, fragment: string;
  try {
    const decoded = decodeURIComponent(raw);
    if (decoded.includes("?") && !nativeAbsolute(decoded)) return null;
    const scoped = absolutePathRelativeToRoot(root, decoded);
    fromRoot = scoped !== null;
    relative = scoped ?? decoded;
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
  const parts = fromRoot ? [] : base.split("/").slice(0, -1);
  for (const part of relative.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") {
      if (!parts.length) return null;
      parts.pop();
    } else parts.push(part);
  }
  if (
    !parts.length ||
    parts.some((part) => {
      const p = part.replace(/[. ]+$/, "").toLowerCase();
      return (
        p === ".git" ||
        p === ".pipeline-tasks" ||
        p.startsWith(".pipeline-apply-")
      );
    })
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
