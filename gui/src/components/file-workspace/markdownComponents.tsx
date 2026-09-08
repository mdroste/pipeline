import type { Components } from "react-markdown";
import { Children, isValidElement } from "react";
import SafeMarkdownLink from "../SafeMarkdownLink";
import HighlightedCode from "./HighlightedCode";
import { MarkdownImage } from "./FileNavigation";

export const fileMarkdownComponents: Components = {
  a: SafeMarkdownLink,
  img: MarkdownImage,
  pre: ({ children }) => {
    const child = Children.toArray(children)[0];
    if (isValidElement<{ children?: unknown; className?: string }>(child)) {
      const language =
        /language-([^\s]+)/.exec(child.props.className ?? "")?.[1] ??
        "plaintext";
      const text = String(child.props.children ?? "").replace(/\n$/, "");
      return <HighlightedCode text={text} language={language} copy />;
    }
    return <pre>{children}</pre>;
  },
};
