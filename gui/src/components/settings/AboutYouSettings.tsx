import { useEffect, useId, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import {
  researcherClient,
  type ResearcherProfile,
  type StyleDraft,
  type WritingKind,
  type WritingSample,
  type WritingSampleUpdate,
} from "../../lib/researcherClient";
import { button, fieldError, input, muted } from "../../ui/classes";
import { confirmDialog } from "../DialogService";
import ReportIssueLink from "../ReportIssueLink";
import { SectionHeader, SettingsCard } from "./controls";
import { SaveFeedback, useSettingsOperation } from "./SaveState";

const KINDS: { value: WritingKind; label: string }[] = [
  { value: "paper", label: "Paper" },
  { value: "grant", label: "Grant proposal" },
  { value: "response", label: "Referee response" },
  { value: "other", label: "Other" },
];
const SAMPLE_EXTENSIONS = ["pdf", "docx", "tex", "md", "txt"];

const message = workbenchErrorMessage;

/**
 * Settings → About you: what assistants know about the researcher in every
 * project. Background is sent with conversations; Your writing keeps finished
 * papers beside it.
 */
export default function AboutYouSettings() {
  const [profile, setProfile] = useState<ResearcherProfile | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    researcherClient
      .profile()
      .then((value) => {
        if (live) setProfile(value);
      })
      .catch((cause) => {
        if (live) setLoadError(message(cause));
      });
    return () => {
      live = false;
    };
  }, []);
  if (loadError)
    return (
      <SettingsCard id="about-you-background">
        <SectionHeader title="Background" />
        <p role="alert" className="settings-error">
          About you could not be loaded: {loadError}
          <ReportIssueLink />
        </p>
      </SettingsCard>
    );
  if (!profile)
    return (
      <SettingsCard id="about-you-background">
        <p role="status" className="text-sm">
          Loading About you…
        </p>
      </SettingsCard>
    );
  return (
    <>
      <SettingsCard id="about-you-background">
        <NoteCard note={BACKGROUND} profile={profile} onSaved={setProfile} />
      </SettingsCard>
      <SettingsCard id="about-you-writing">
        <Writing profile={profile} onChanged={setProfile} />
      </SettingsCard>
      <SettingsCard id="about-you-style">
        <NoteCard note={STYLE} profile={profile} onSaved={setProfile} />
      </SettingsCard>
    </>
  );
}

interface NoteCopy {
  note: "about" | "style";
  title: string;
  description: string;
  placeholder: string;
  hint?: string;
  save: string;
  /** Offer a model-written draft from the papers marked as style examples. */
  draftable?: boolean;
}

const BACKGROUND: NoteCopy = {
  note: "about",
  title: "Background",
  description: "Your field, the kind of work you do, and who you write for.",
  placeholder:
    "Labor economist. Applied micro with administrative data, mostly difference-in-differences and event studies. I write for general-interest economics journals. British spelling.",
  hint: "Sent with every conversation as additional context. To see what an assistant receives, choose Context in a conversation’s menu.",
  save: "Save background",
};

const STYLE: NoteCopy = {
  note: "style",
  title: "Style notes",
  description: "How you write, in instructions an assistant can follow.",
  placeholder:
    "First person plural. Short declarative openings. State the estimate and its units before interpreting it. Hedge with “suggests”, not “may potentially”. No bullet points in the main text.",
  save: "Save style notes",
  draftable: true,
};

function NoteCard({
  note,
  profile,
  onSaved,
}: {
  note: NoteCopy;
  profile: ResearcherProfile;
  onSaved: (profile: ResearcherProfile) => void;
}) {
  const fieldId = useId();
  const hintId = useId();
  const savedText = profile[note.note];
  const [draft, setDraft] = useState(savedText);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const trimmed = draft.trim();
  const changed = trimmed !== savedText.trim();
  const tooLong =
    new TextEncoder().encode(trimmed).length > profile.maxNoteBytes;
  // An edit that has not been saved holds navigation, like an unsaved key.
  useSettingsOperation({ pending: changed || saving, error });
  const save = async () => {
    setSaving(true);
    setSaved(false);
    setError(null);
    try {
      const next = await researcherClient.saveNote({
        note: note.note,
        text: trimmed,
      });
      onSaved(next);
      setDraft(next[note.note]);
      setSaved(true);
    } catch (cause) {
      setError(`Could not save ${note.title}: ${message(cause)}`);
    } finally {
      setSaving(false);
    }
  };
  return (
    <>
      <SectionHeader title={note.title} description={note.description} />
      <label htmlFor={fieldId} className="sr-only">
        {note.title}
      </label>
      <textarea
        id={fieldId}
        className={input}
        rows={6}
        value={draft}
        aria-describedby={tooLong || note.hint ? hintId : undefined}
        aria-invalid={tooLong || undefined}
        placeholder={note.placeholder}
        onChange={(event) => {
          setDraft(event.target.value);
          setSaved(false);
        }}
      />
      {tooLong ? (
        <p id={hintId} role="alert" className={fieldError}>
          Shorten this to 4 KiB or less, about 4,000 characters.
        </p>
      ) : (
        note.hint && (
          <p id={hintId} className={`${muted} mt-1`}>
            {note.hint}
          </p>
        )
      )}
      <div className="mt-2 flex flex-wrap items-center gap-3">
        <button
          type="button"
          className={button}
          disabled={saving || !changed || tooLong}
          onClick={() => void save()}
        >
          {note.save}
        </button>
        {changed && !saving && (
          <button
            type="button"
            className={button}
            onClick={() => {
              setDraft(savedText);
              setError(null);
            }}
          >
            Cancel
          </button>
        )}
        <SaveFeedback
          pending={saving}
          error={error}
          saved={saved && !changed}
          retry={error ? () => void save() : undefined}
        />
      </div>
      {note.draftable && (
        <StyleDraftPanel
          profile={profile}
          onUse={(text) => {
            setDraft(text);
            setSaved(false);
            setError(null);
          }}
        />
      )}
    </>
  );
}

/**
 * Asks the default model for style notes drafted from the papers marked as
 * examples, and shows the draft for the researcher to use or discard. Using
 * it fills the notes field; nothing is saved until they choose to.
 */
function StyleDraftPanel({
  profile,
  onUse,
}: {
  profile: ResearcherProfile;
  onUse: (text: string) => void;
}) {
  const [drafting, setDrafting] = useState(false);
  const [result, setResult] = useState<StyleDraft | null>(null);
  const [error, setError] = useState<string | null>(null);
  const examples = profile.samples.filter(
    (sample) => sample.useAsExample && sample.words !== null,
  ).length;
  const request = async () => {
    setDrafting(true);
    setError(null);
    setResult(null);
    try {
      setResult(await researcherClient.draftStyle());
    } catch (cause) {
      setError(`Could not draft style notes: ${message(cause)}`);
    } finally {
      setDrafting(false);
    }
  };
  return (
    <div className="mt-4">
      <div className="flex flex-wrap items-center gap-3">
        <button
          type="button"
          className={button}
          disabled={drafting || examples === 0}
          onClick={() => void request()}
        >
          {drafting ? "Drafting…" : "Draft from my papers"}
        </button>
        <span className={muted} role={drafting ? "status" : undefined}>
          {examples === 0
            ? "Mark a paper in Your writing as a style example to draft from it."
            : drafting
              ? "Reading your papers with your default model. This can take a minute."
              : "Uses your default model and the papers marked as style examples."}
        </span>
      </div>
      {error && (
        <p role="alert" className="settings-error mt-2">
          {error}
        </p>
      )}
      {result && (
        <section
          aria-label="Drafted style notes"
          className="mt-3 rounded-ui-md border border-line p-3"
        >
          <p className={muted}>
            Drafted from{" "}
            {result.papers.length === 1
              ? "1 paper"
              : `${result.papers.length} papers`}
            : {result.papers.join("; ")}.
          </p>
          <p className="mt-2 whitespace-pre-wrap text-sm">{result.text}</p>
          <div className="mt-3 flex flex-wrap gap-3">
            <button
              type="button"
              className={button}
              onClick={() => {
                onUse(result.text);
                setResult(null);
              }}
            >
              Use draft
            </button>
            <button
              type="button"
              className={button}
              onClick={() => setResult(null)}
            >
              Discard
            </button>
          </div>
        </section>
      )}
    </div>
  );
}

function Writing({
  profile,
  onChanged,
}: {
  profile: ResearcherProfile;
  onChanged: (profile: ResearcherProfile) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [errors, setErrors] = useState<string[]>([]);
  useSettingsOperation({ pending: busy });
  const full = profile.samples.length >= profile.maxSamples;
  const add = async () => {
    setErrors([]);
    let chosen: string | string[] | null;
    try {
      chosen = await open({
        title: "Add to Your writing",
        multiple: true,
        filters: [{ name: "Papers", extensions: SAMPLE_EXTENSIONS }],
      });
    } catch (cause) {
      setErrors([message(cause)]);
      return;
    }
    const paths = Array.isArray(chosen) ? chosen : chosen ? [chosen] : [];
    if (!paths.length) return;
    setBusy(true);
    const failed: string[] = [];
    // One at a time: each file is copied and read before the next starts.
    for (const path of paths) {
      try {
        onChanged(await researcherClient.addSample({ path }));
      } catch (cause) {
        const name = path.split(/[\\/]/).pop() ?? path;
        failed.push(`${name}: ${message(cause)}`);
      }
    }
    setErrors(failed);
    setBusy(false);
  };
  const run = async (work: () => Promise<ResearcherProfile>) => {
    setErrors([]);
    setBusy(true);
    try {
      onChanged(await work());
      return true;
    } catch (cause) {
      setErrors([message(cause)]);
      return false;
    } finally {
      setBusy(false);
    }
  };
  const remove = async (sample: WritingSample) => {
    const confirmed = await confirmDialog(
      `Remove “${sample.title}” from Your writing? Pipeline deletes its copy. Your own file isn’t touched.`,
      { confirmLabel: "Remove", destructive: true },
    );
    if (confirmed)
      await run(() => researcherClient.removeSample({ id: sample.id }));
  };
  return (
    <>
      <SectionHeader
        title="Your writing"
        description="Finished papers and other writing, kept as examples of how you write."
      />
      {profile.samples.length > 0 && (
        <ul className="mt-3 space-y-3" aria-label="Your writing">
          {profile.samples.map((sample) => (
            <SampleRow
              key={sample.id}
              sample={sample}
              disabled={busy}
              onUpdate={(update) =>
                run(() => researcherClient.updateSample({ update }))
              }
              onRemove={() => void remove(sample)}
            />
          ))}
        </ul>
      )}
      <div className="mt-3 flex flex-wrap items-center gap-3">
        <button
          type="button"
          className={button}
          disabled={busy || full}
          onClick={() => void add()}
        >
          {busy ? "Working…" : "Add paper…"}
        </button>
        <span className={muted}>
          {full
            ? `Your writing holds up to ${profile.maxSamples} items. Remove one to add another.`
            : "PDF, Word, TeX, Markdown, or text."}
        </span>
      </div>
      {errors.length > 0 && (
        <div role="alert" className="settings-error mt-2">
          {errors.map((error) => (
            <p key={error}>{error}</p>
          ))}
        </div>
      )}
      <p className={`${muted} mt-3 break-all`}>
        About you is kept in {profile.folder}.{" "}
        <button
          type="button"
          className="underline"
          onClick={() => {
            void researcherClient
              .openFolder()
              .catch((cause) => setErrors([message(cause)]));
          }}
        >
          Open folder
        </button>
      </p>
    </>
  );
}

function SampleRow({
  sample,
  disabled,
  onUpdate,
  onRemove,
}: {
  sample: WritingSample;
  disabled: boolean;
  onUpdate: (update: WritingSampleUpdate) => Promise<boolean>;
  onRemove: () => void;
}) {
  const [title, setTitle] = useState(sample.title);
  const [year, setYear] = useState(sample.year ? String(sample.year) : "");
  const savedYear = sample.year ? String(sample.year) : "";
  const yearValue = year.trim() ? Number(year) : null;
  const yearInvalid =
    yearValue !== null &&
    !(Number.isInteger(yearValue) && yearValue >= 1900 && yearValue <= 2200);
  const titleInvalid = !title.trim();
  const save = async (change: Partial<WritingSampleUpdate>) => {
    const ok = await onUpdate({
      id: sample.id,
      title: sample.title,
      year: sample.year,
      kind: sample.kind,
      useAsExample: sample.useAsExample,
      ...change,
    });
    if (!ok) {
      setTitle(sample.title);
      setYear(savedYear);
    }
  };
  const details = [
    sample.words === null
      ? "No text found"
      : `${sample.words.toLocaleString()} words`,
    sample.fileName,
  ].join(" · ");
  return (
    <li className="space-y-1">
      <div className="flex flex-wrap items-center gap-2">
        <input
          className={`${input} min-w-[12rem] flex-1`}
          style={{ width: "auto" }}
          aria-label={`Title of ${sample.title}`}
          aria-invalid={titleInvalid || undefined}
          value={title}
          disabled={disabled}
          onChange={(event) => setTitle(event.target.value)}
          onBlur={() => {
            if (titleInvalid) setTitle(sample.title);
            else if (title.trim() !== sample.title)
              void save({ title: title.trim() });
          }}
        />
        <select
          className="ui-input"
          aria-label={`Kind of ${sample.title}`}
          value={sample.kind}
          disabled={disabled}
          onChange={(event) =>
            void save({ kind: event.target.value as WritingKind })
          }
        >
          {KINDS.map((kind) => (
            <option key={kind.value} value={kind.value}>
              {kind.label}
            </option>
          ))}
        </select>
        <input
          className="ui-input"
          style={{ width: "5.5rem" }}
          inputMode="numeric"
          placeholder="Year"
          aria-label={`Year of ${sample.title}`}
          aria-invalid={yearInvalid || undefined}
          value={year}
          disabled={disabled}
          onChange={(event) => setYear(event.target.value)}
          onBlur={() => {
            if (yearInvalid) setYear(savedYear);
            else if (year.trim() !== savedYear) void save({ year: yearValue });
          }}
        />
        <button
          type="button"
          className={button}
          disabled={disabled}
          aria-label={`Remove ${sample.title}`}
          onClick={onRemove}
        >
          Remove
        </button>
      </div>
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
        <label className="flex items-center gap-2 text-xs">
          <input
            type="checkbox"
            checked={sample.useAsExample}
            disabled={disabled || sample.words === null}
            onChange={(event) =>
              void save({ useAsExample: event.target.checked })
            }
          />
          Use as style example
        </label>
        <p className={`${muted} break-all`}>{details}</p>
      </div>
    </li>
  );
}
