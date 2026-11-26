import {
  createContext,
  useContext,
  useMemo,
  useState,
  type ReactNode,
  type ComponentPropsWithRef,
} from "react";

interface Surface {
  target: HTMLDivElement | null;
  setTarget: (target: HTMLDivElement | null) => void;
  page: "home" | "main" | "workspace" | null;
  setPage: (page: "home" | "main" | "workspace" | null) => void;
  projectNavigation: ReactNode;
  setProjectNavigation: (navigation: ReactNode) => void;
}
const Context = createContext<Surface | null>(null);
export const useExampleReviewSurface = () => useContext(Context);

export function ExampleReviewSurfaceProvider({
  children,
}: {
  children: ReactNode;
}) {
  const [target, setTarget] = useState<HTMLDivElement | null>(null);
  const [page, setPage] = useState<Surface["page"]>(null);
  const [projectNavigation, setProjectNavigation] = useState<ReactNode>(null);
  const value = useMemo(
    () => ({
      target,
      setTarget,
      page,
      setPage,
      projectNavigation,
      setProjectNavigation,
    }),
    [target, page, projectNavigation],
  );
  return <Context.Provider value={value}>{children}</Context.Provider>;
}

/** Keep the user's real page mounted and unchanged beneath the tutorial. */
export function ExampleReviewSurface({
  children,
  ...attributes
}: ComponentPropsWithRef<"div">) {
  const surface = useExampleReviewSurface();
  if (!surface) return <div {...attributes}>{children}</div>;
  return (
    <div {...attributes}>
      <div
        style={{ display: surface.page ? "none" : "contents" }}
        inert={surface.page ? true : undefined}
      >
        {children}
      </div>
      <div
        ref={surface.setTarget}
        hidden={!surface.page}
        className="h-full min-h-0"
      />
    </div>
  );
}
