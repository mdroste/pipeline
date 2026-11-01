// Syntax colors for the source editor. They are CSS variables, set in files.css,
// so light and dark themes change with the app.
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";

export const editorHighlight = syntaxHighlighting(
  HighlightStyle.define([
    {
      tag: [t.keyword, t.operatorKeyword, t.modifier],
      color: "var(--file-syntax-keyword)",
    },
    {
      tag: [
        t.tagName,
        t.typeName,
        t.className,
        t.namespace,
        t.macroName,
        t.function(t.variableName),
        t.function(t.propertyName),
        t.definition(t.variableName),
        t.special(t.variableName),
        t.standard(t.variableName),
        t.attributeName,
      ],
      color: "var(--file-syntax-type)",
    },
    {
      tag: [t.string, t.special(t.string), t.regexp, t.attributeValue],
      color: "var(--file-syntax-string)",
    },
    {
      tag: [t.number, t.bool, t.null, t.atom],
      color: "var(--file-syntax-number)",
    },
    {
      tag: [t.comment, t.lineComment, t.blockComment],
      color: "var(--file-syntax-comment)",
      fontStyle: "italic",
    },
    {
      tag: [t.meta, t.processingInstruction],
      color: "var(--file-syntax-comment)",
    },
    { tag: t.heading, fontWeight: "600" },
    { tag: t.strong, fontWeight: "600" },
    { tag: t.emphasis, fontStyle: "italic" },
    { tag: t.link, textDecoration: "underline" },
  ]),
);
