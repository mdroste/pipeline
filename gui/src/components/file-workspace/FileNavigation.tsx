import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
  type ImgHTMLAttributes,
} from "react";
import { resolveFileLink, type FileLocation } from "../../lib/fileLinks";

export interface FileNavigation {
  path: string;
  open: (location: FileLocation) => void | Promise<void>;
  image: (path: string) => Promise<string>;
}
export const FileNavigationContext = createContext<FileNavigation | null>(null);
export const useFileNavigation = () => useContext(FileNavigationContext);

export function FileNavigationScope({ value, children }: { value: FileNavigation; children: ReactNode }) {
  const stable = useMemo(() => value, [value.path, value.open, value.image]);
  return <FileNavigationContext.Provider value={stable}>{children}</FileNavigationContext.Provider>;
}

export function MarkdownImage({
  src,
  alt,
  node: _node,
  ...props
}: ImgHTMLAttributes<HTMLImageElement> & { node?: unknown }) {
  const navigation = useFileNavigation();
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    setUrl(null);
    setError("");
    const location =
      typeof src === "string" && navigation
        ? resolveFileLink(navigation.path, src)
        : null;
    if (!location || !navigation) {
      setError("Image is not available in this document’s files.");
      return;
    }
    void navigation
      .image(location.path)
      .then((value) => {
        if (live) setUrl(value);
      })
      .catch(() => {
        if (live) setError(`Image unavailable: ${src}`);
      });
    return () => {
      live = false;
    };
  }, [src, navigation]);
  if (url) return <img {...props} src={url} alt={alt} />;
  return (
    <span className="file-missing-image" title={error}>
      {alt || "Image"} · {error || "Loading…"}
    </span>
  );
}
