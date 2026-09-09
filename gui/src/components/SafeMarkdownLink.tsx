import type {
  AnchorHTMLAttributes,
  MouseEvent as ReactMouseEvent,
} from "react";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { notify } from "./DialogService";
import { useFileNavigation } from "./file-workspace/FileNavigation";
import { absoluteLocalFilePath, resolveFileLink } from "../lib/fileLinks";

type SafeMarkdownLinkProps = AnchorHTMLAttributes<HTMLAnchorElement> & {
  node?: unknown;
};

const EXTERNAL_PROTOCOLS = new Set(["http:", "https:", "mailto:"]);

/**
 * Model-authored Markdown is untrusted navigation input. Only links that can
 * safely be handed to the operating system are treated as external links.
 */
export function safeExternalHref(href: string | undefined): string | null {
  const candidate = href?.trim();
  if (!candidate) return null;

  try {
    const parsed = new URL(candidate);
    if (!EXTERNAL_PROTOCOLS.has(parsed.protocol.toLowerCase())) return null;
    if (
      (parsed.protocol === "http:" || parsed.protocol === "https:") &&
      !parsed.hostname
    ) {
      return null;
    }
    return candidate;
  } catch {
    return null;
  }
}

function openFromDesktop(
  event: ReactMouseEvent<HTMLAnchorElement>,
  href: string,
) {
  event.preventDefault();
  void openExternal(href).catch((error) => {
    console.error("Could not open external Markdown link:", error);
    notify(
      `Could not open the external link: ${
        error instanceof Error ? error.message : String(error)
      }`,
    );
  });
}

/**
 * ReactMarkdown anchor renderer that keeps external navigation out of the
 * application WebView. Same-document fragments remain ordinary anchors so
 * footnotes continue to work; all other relative or unsafe URLs render inert.
 */
export default function SafeMarkdownLink({
  href,
  children,
  node: _node,
  onClick,
  onAuxClick,
  className,
  title,
  ...props
}: SafeMarkdownLinkProps) {
  const navigation = useFileNavigation();
  if (href?.startsWith("#")) {
    return (
      <a
        {...props}
        className={className}
        title={title}
        href={href}
        onClick={onClick}
        onAuxClick={onAuxClick}
      >
        {children}
      </a>
    );
  }

  const externalHref = safeExternalHref(href);
  if (!externalHref) {
    const location =
      href && navigation
        ? resolveFileLink(navigation.path, href, navigation.root)
        : null;
    if (location && navigation)
      return (
        <a
          {...props}
          className={className}
          title={title ?? `Open ${location.path} in Pipeline`}
          href={href}
          onClick={(event) => {
            event.preventDefault();
            void Promise.resolve(navigation.open(location)).catch((error) =>
              notify(`Could not open this file: ${String(error)}`),
            );
          }}
        >
          {children}
        </a>
      );
    const absolutePath = href ? absoluteLocalFilePath(href) : null;
    const openAbsolute = navigation?.openAbsolute;
    if (absolutePath && openAbsolute)
      return (
        <a
          {...props}
          className={className}
          title={title ?? "Open this conversation file"}
          href={href}
          onClick={(event) => {
            event.preventDefault();
            void Promise.resolve(openAbsolute(absolutePath)).catch((error) =>
              notify(`Could not open this file: ${String(error)}`),
            );
          }}
        >
          {children}
        </a>
      );
    return (
      <span
        className={className}
        title={title ?? "This link is unavailable in the document’s files."}
      >
        {children}
      </span>
    );
  }

  return (
    <a
      {...props}
      className={className}
      title={title}
      href={externalHref}
      onClick={(event) => {
        onClick?.(event);
        if (!event.defaultPrevented) openFromDesktop(event, externalHref);
      }}
      onAuxClick={(event) => {
        onAuxClick?.(event);
        // Treat a middle-click like an ordinary external open, but leave a
        // right-click available for the platform's context menu.
        if (!event.defaultPrevented && event.button === 1) {
          openFromDesktop(event, externalHref);
        }
      }}
    >
      {children}
    </a>
  );
}
