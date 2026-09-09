interface HtmlNode {
  properties?: Record<string, unknown>;
  children?: HtmlNode[];
}

/** Scope rendered identities (including generated footnotes) to one reader.
 * Canonical fragments stay available for source-file navigation and exports.
 */
export function reportAnchorPlugin(prefix: string) {
  return () => (tree: unknown) => {
    const visit = (node: HtmlNode) => {
      const props = node.properties;
      if (props) {
        if (typeof props.id === "string") {
          props.dataSourceAnchor = props.id;
          props.id = prefix + props.id;
        }
        if (typeof props.href === "string" && props.href.startsWith("#")) {
          props.href = `#${prefix}${props.href.slice(1)}`;
        }
        for (const key of ["ariaDescribedBy", "ariaLabelledBy"]) {
          const value = props[key];
          if (Array.isArray(value))
            props[key] = value.map((id) => prefix + String(id));
          else if (typeof value === "string")
            props[key] = value
              .split(/\s+/)
              .map((id) => prefix + id)
              .join(" ");
        }
      }
      node.children?.forEach(visit);
    };
    visit(tree as HtmlNode);
  };
}
