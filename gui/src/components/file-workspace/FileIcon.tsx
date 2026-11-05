// The small type icon beside a file's name in the tree, its tab, and search
// results. It reads the name only, so a listing never has to open the file.
import { Icon, type IconName } from "../../ui/icons";

type FileType = "pdf" | "code" | "table" | "image" | "media" | "text";

const TYPES: Record<Exclude<FileType, "text">, RegExp> = {
  pdf: /\.pdf$/i,
  code: /\.(tex|bib|sty|cls|do|ado|py|r|jl|m|sh|js|ts|tsx|json|ya?ml|toml|html?|css|ipynb)$/i,
  table: /\.(csv|tsv|xlsx|xlsm|dta)$/i,
  image: /\.(png|jpe?g|gif|webp|bmp|avif|svg|tiff?)$/i,
  media: /\.(mp3|wav|m4a|flac|ogg|mp4|mov|webm|mkv)$/i,
};
const ICONS: Record<FileType, IconName> = {
  pdf: "file",
  code: "code",
  table: "table",
  image: "image",
  media: "media",
  text: "file",
};

export function fileType(name: string): FileType {
  for (const [type, pattern] of Object.entries(TYPES))
    if (pattern.test(name)) return type as FileType;
  return "text";
}

export default function FileIcon({ name }: { name: string }) {
  const type = fileType(name);
  return (
    <span className="file-icon" data-type={type}>
      <Icon name={ICONS[type]} className="h-4 w-4" />
    </span>
  );
}
