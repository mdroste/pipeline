import type {
  AnchorHTMLAttributes,
  MouseEvent as ReactMouseEvent,
} from "react";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { notify } from "./DialogService";
import { useFileNavigation } from "./file-workspace/FileNavigation";
import { absoluteLocalFileLocation, resolveFileLink } from "../lib/fileLinks";
import { workbenchErrorMessage } from "../lib/workbenchError";

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
 * application WebView. Same-document fragments scroll to their target without
 * touching the window hash, which holds the app's route; all other relative or
 * unsafe URLs render inert.
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
        onClick={(event) => {
          onClick?.(event);
          if (event.defaultPrevented) return;
          event.preventDefault();
          document
            .getElementById(href.slice(1))
            ?.scrollIntoView({ block: "center" });
        }}
        onAuxClick={onAuxClick}
      >
        {children}
      </a>
    );
  }

  const externalHref = safeExternalHref(href);
  if (!externalHref) {
    const scoped =
      href && navigation
        ? resolveFileLink(navigation.path, href, navigation.root)
        : null;
    const open = scoped ? navigation?.open : navigation?.openAbsolute;
    const location =
      scoped ?? (href && open ? absoluteLocalFileLocation(href) : null);
    if (location && open) {
      const openFile = (event: ReactMouseEvent<HTMLAnchorElement>) => {
        event.preventDefault();
        void Promise.resolve(open(location)).catch((error) =>
          notify(`Could not open this file: ${workbenchErrorMessage(error)}`),
        );
      };
      return (
        <a
          {...props}
          className={className}
          title={title ?? `Open ${location.path} in Pipeline`}
          href={href}
          onClick={openFile}
          onAuxClick={(event) => {
            if (event.button === 1) openFile(event);
          }}
        >
          {children}
        </a>
      );
    }
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
