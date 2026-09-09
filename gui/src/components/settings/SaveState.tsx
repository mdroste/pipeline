import {
  createContext,
  useContext,
  useEffect,
  useId,
  type ReactNode,
} from "react";

export interface SaveState {
  pending?: boolean;
  error?: string | null;
  saved?: boolean;
}
export const SettingsCardId = createContext("");
export const SettingsOperations = createContext<
  (id: string, state: SaveState | null) => void
>(() => {});
export const ReviewSaveState = createContext<
  SaveState & { retry?: () => void; scope?: string }
>({});

/** Aggregate pending operations for navigation, without sharing writable stores. */
export function useSettingsOperation(state: SaveState) {
  const report = useContext(SettingsOperations);
  const id = useId();
  useEffect(() => {
    report(id, state);
    return () => report(id, null);
  }, [report, id, state.pending, state.error]);
}

export function SaveFeedback({
  pending,
  error,
  saved,
  retry,
}: SaveState & { retry?: () => void }) {
  if (error)
    return (
      <p role="alert" className="settings-error">
        {error}
        {retry && (
          <>
            {" "}
            <button
              type="button"
              className="settings-text-link"
              onClick={retry}
            >
              Retry
            </button>
          </>
        )}
      </p>
    );
  if (!pending && !saved) return null;
  return (
    <p role="status" className="settings-inline-status">
      {pending ? "Saving changes…" : "Saved"}
    </p>
  );
}

/** Put Review save feedback at the section where the edit originated. */
export function ReviewSaveFeedback() {
  const state = useContext(ReviewSaveState);
  const card = useContext(SettingsCardId);
  if (state.scope && card !== state.scope) return null;
  return <SaveFeedback {...state} />;
}

export function SettingRow({
  id,
  label,
  description,
  children,
}: {
  id?: string;
  label: string;
  description?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div
      id={id}
      tabIndex={id ? -1 : undefined}
      className="settings-row settings-anchor"
    >
      <div className="settings-row-copy">
        <div className="settings-row-label">{label}</div>
        {description && (
          <p className="settings-row-description">{description}</p>
        )}
      </div>
      <div className="settings-row-control">{children}</div>
    </div>
  );
}
