/**
 * A native path as people write it. Windows project roots are canonical and
 * keep the verbatim prefix (`\\?\C:\…`, `\\?\UNC\server\share\…`) that the
 * backend compares against; show them as `C:\…` and `\\server\share\…`.
 * Pass the stored value, not this one, back to the backend.
 */
export function displayPath(path: string): string;
export function displayPath(path: string | null | undefined): string | null;
export function displayPath(path: string | null | undefined): string | null {
  if (path == null) return null;
  if (/^\\\\\?\\UNC\\/i.test(path)) return `\\\\${path.slice(8)}`;
  if (/^\\\\\?\\[a-z]:(\\|$)/i.test(path)) return path.slice(4);
  return path;
}
