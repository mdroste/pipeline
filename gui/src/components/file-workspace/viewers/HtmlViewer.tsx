// A rendered HTML file with scripts, forms, navigation and network requests all
// off: the empty sandbox plus the response's own Content-Security-Policy.
export default function HtmlViewer({
  src,
  title,
  stale,
}: {
  src: string;
  title: string;
  stale: boolean;
}) {
  return (
    <div className="file-html">
      <p role="note" className="file-status">
        {stale ? "Showing the saved file, not your draft. " : ""}
        Scripts and online content are off here. Open externally for interactive
        pages.
      </p>
      <iframe sandbox="" src={src} title={`Rendered ${title}`} />
    </div>
  );
}
