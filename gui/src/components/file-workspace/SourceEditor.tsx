import { useAppPreferences } from "../../lib/appPreferences";
import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import { basicSetup } from "codemirror";
import { Annotation, Compartment, EditorState, Text } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { indentWithTab } from "@codemirror/commands";
import { StreamLanguage, type StreamParser } from "@codemirror/language";
import {
  autocompletion,
  type CompletionContext,
} from "@codemirror/autocomplete";
import { languages } from "@codemirror/language-data";
import { stex } from "@codemirror/legacy-modes/mode/stex";
import { octave } from "@codemirror/legacy-modes/mode/octave";
import {
  FILE_LANGUAGES,
  fileLanguage,
  texCompletions,
} from "../../lib/fileLanguages";
import { editorHighlight } from "./editorHighlight";
import "./files.css";

// Stata's macro quoting and line/block comments need a dedicated mode.
export const stataMode: StreamParser<{ block: boolean }> = {
  startState: () => ({ block: false }),
  token(stream, state) {
    if (state.block) {
      if (stream.skipTo("*/")) {
        stream.match("*/");
        state.block = false;
      } else stream.skipToEnd();
      return "comment";
    }
    if (stream.eatSpace()) return null;
    if (stream.match("/*")) {
      state.block = true;
      return "comment";
    }
    if (
      (stream.string.slice(0, stream.pos).trim() === "" &&
        stream.match(/\*.*/)) ||
      stream.match(/\/\/.*$/)
    )
      return "comment";
    if (stream.match(/`[^']*'/) || stream.match(/\$\{?\w+\}?/))
      return "variableName.special";
    if (stream.match(/"(?:[^"\\]|\\.)*"/)) return "string";
    if (stream.match(/(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?\b/i))
      return "number";
    if (
      stream.match(
        /\b(?:regress|reg|ivregress|xtreg|reghdfe|gen|generate|replace|egen|summarize|sum|use|save|merge|append|keep|drop|if|else|foreach|forvalues|while|local|global|scalar|matrix|program|end|return|ereturn|capture|quietly|noisily|preserve|restore|import|export|clear|set|do|include|exit|estimates|test|predict|margins|log|display|di|assert|sort|bysort|by|collapse|reshape|tsset|xtset|tabulate|tab|tabstat|table|label|rename|ren|recode|destring|tostring|encode|decode|describe|des|desc|list|count|duplicates|order|format|correlate|corr|pwcorr|logit|probit|logistic|ologit|oprobit|mlogit|poisson|nbreg|tobit|areg|qreg|ivreg2|xtivreg|xtlogit|xtpoisson|sureg|ttest|lincom|nlcom|testparm|bootstrap|simulate|permute|svy|svyset|stset|stcox|streg|eststo|esttab|estout|outreg2|graph|twoway|scatter|line|histogram|kdensity|binscatter|coefplot|levelsof|contract|expand|joinby|fillin|version|syntax|args|tempvar|tempfile|tempname|mata|continue|cd|ssc|run|using|in|of)\b/,
      )
    )
      return "keyword";
    if (stream.match(/[A-Za-z_]\w*/)) return "variableName";
    stream.next();
    return null;
  },
};
/** Marks text the owner handed in, which `onChange` must not report back. */
const fromOwner = Annotation.define<boolean>();
const surrogate = (code: number, low: boolean) =>
  code >= (low ? 0xdc00 : 0xd800) && code <= (low ? 0xdfff : 0xdbff);
/** The smallest replacement that turns `current` into `next`, or null. */
function changedSpan(current: string, next: string) {
  if (current === next) return null;
  const shorter = Math.min(current.length, next.length);
  let from = 0;
  while (from < shorter && current[from] === next[from]) from++;
  if (from > 0 && surrogate(current.charCodeAt(from - 1), false)) from--;
  let same = 0;
  while (
    same < shorter - from &&
    current[current.length - 1 - same] === next[next.length - 1 - same]
  )
    same++;
  if (same > 0 && surrogate(current.charCodeAt(current.length - same), true))
    same--;
  return {
    from,
    to: current.length - same,
    insert: next.slice(from, next.length - same),
  };
}
export interface SourceEditorHandle {
  goToLine: (line: number) => void;
  focus: () => void;
}
/**
 * A request to reveal a source line. Each new object is applied once, without
 * moving focus, so keep the same object between renders. An explicit user
 * action that should focus the editor uses the handle's goToLine instead.
 */
export interface SourceLineRequest {
  line?: number;
}
interface Props {
  value: string;
  path: string;
  onChange?: (text: string) => void;
  readOnly?: boolean;
  label?: string;
  onSave?: () => void;
  reveal?: SourceLineRequest | null;
  completionTexts?: string[];
  /** Wrap long lines unless the researcher turns it off, for prose. */
  wrapByDefault?: boolean;
  onSelection?: (selection: {
    text: string;
    start: number;
    end: number;
    line: number;
  }) => void;
}

const SourceEditor = forwardRef<SourceEditorHandle, Props>(
  function SourceEditor(props, ref) {
    const {
      value,
      path,
      readOnly = false,
      label = "Source editor",
      completionTexts = [],
    } = props;
    const host = useRef<HTMLDivElement>(null);
    const view = useRef<EditorView | null>(null);
    const current = useRef(props);
    current.current = props;
    const [language, setLanguage] = useState(() => fileLanguage(path));
    const { editorWrap, editorFontSize } = useAppPreferences();
    const [wrapOverride, setWrap] = useState<boolean | null>(null);
    const wrap = wrapOverride ?? (props.wrapByDefault || editorWrap);
    const [languageError, setLanguageError] = useState("");
    const [cursor, setCursor] = useState({ line: 1, column: 1 });
    const echoes = useRef<string[]>([]);
    const [configuration] = useState(() => ({
      language: new Compartment(),
      wrap: new Compartment(),
      readonly: new Compartment(),
      completion: new Compartment(),
      lineSeparator: new Compartment(),
    }));
    useEffect(() => {
      host.current?.style.setProperty(
        "--source-editor-font-size",
        `${editorFontSize}px`,
      );
    }, [editorFontSize]);
    const goToLine = (n: number, { focus = true } = {}) => {
      const editor = view.current;
      if (!editor) return;
      const line = editor.state.doc.line(
        Math.max(1, Math.min(Math.floor(n) || 1, editor.state.doc.lines)),
      );
      editor.dispatch({
        selection: { anchor: line.from },
        effects: EditorView.scrollIntoView(line.from, { y: "center" }),
      });
      if (focus) editor.focus();
    };
    useImperativeHandle(ref, () => ({
      goToLine: (line) => goToLine(line),
      focus: () => view.current?.focus(),
    }));
    useEffect(() => {
      if (!host.current) return;
      const editor = new EditorView({
        parent: host.current,
        state: EditorState.create({
          doc: value,
          extensions: [
            basicSetup,
            editorHighlight,
            configuration.lineSeparator.of(
              EditorState.lineSeparator.of(
                value.match(/\r\n|\r|\n/)?.[0] ?? "\n",
              ),
            ),
            keymap.of([
              indentWithTab,
              {
                key: "Mod-s",
                run: () => {
                  current.current.onSave?.();
                  return true;
                },
              },
            ]),
            EditorView.contentAttributes.of({
              "aria-label": label,
              "aria-multiline": "true",
              spellcheck: "false",
            }),
            configuration.language.of([]),
            configuration.wrap.of([]),
            configuration.completion.of([]),
            configuration.readonly.of(EditorState.readOnly.of(readOnly)),
            EditorView.updateListener.of((update) => {
              if (
                update.transactions.some(
                  (edit) => edit.docChanged && !edit.annotation(fromOwner),
                )
              ) {
                const text = update.state.sliceDoc();
                echoes.current = [...echoes.current.slice(-63), text];
                current.current.onChange?.(text);
              }
              if (update.selectionSet) {
                const s = update.state.selection.main;
                const line = update.state.doc.lineAt(s.head);
                setCursor({
                  line: line.number,
                  column: s.head - line.from + 1,
                });
                current.current.onSelection?.({
                  text: update.state.sliceDoc(s.from, s.to),
                  // Callers consume offsets in the original serialized source.
                  start: update.state.sliceDoc(0, s.from).length,
                  end: update.state.sliceDoc(0, s.to).length,
                  line: update.state.doc.lineAt(s.from).number,
                });
              }
            }),
            EditorView.theme({
              "&": {
                height: "100%",
                fontSize: "var(--source-editor-font-size, 13px)",
              },
              ".cm-scroller": {
                overflow: "auto",
                fontFamily: "ui-monospace, monospace",
              },
              ".cm-content": { minHeight: "240px" },
            }),
          ],
        }),
      });
      view.current = editor;
      try {
        const position = Number(
          localStorage.getItem(`pipeline.editor.position.${path}`),
        );
        if (position) editor.scrollDOM.scrollTop = position;
      } catch {
        /* Optional layout. */
      }
      return () => {
        try {
          localStorage.setItem(
            `pipeline.editor.position.${path}`,
            String(editor.scrollDOM.scrollTop),
          );
        } catch {
          /* Optional layout. */
        }
        editor.destroy();
        view.current = null;
      };
    }, [configuration, path]);
    useEffect(() => {
      setLanguage(fileLanguage(path));
    }, [path]);
    useEffect(() => {
      const editor = view.current;
      if (!editor) return;
      // The owner hands back each edit after it renders, and typing can outrun
      // that. An older copy of our own text must not undo the newer keystrokes.
      const echo = echoes.current.indexOf(value);
      if (echo >= 0) {
        echoes.current.splice(0, echo + 1);
        return;
      }
      echoes.current = [];
      if (value === editor.state.sliceDoc()) return;
      // Another editor on the same file, a reload or a discarded draft changes
      // the value. Replace only the differing span so this editor keeps its
      // scroll position and selection.
      const change = changedSpan(
        editor.state.doc.toString(),
        value.replace(/\r\n?/g, "\n"),
      );
      const separator =
        value.match(/\r\n|\r|\n/)?.[0] ?? editor.state.lineBreak;
      if (!change && separator === editor.state.lineBreak) return;
      editor.dispatch({
        annotations: fromOwner.of(true),
        ...(change && {
          changes: { ...change, insert: Text.of(change.insert.split("\n")) },
        }),
        ...(separator !== editor.state.lineBreak && {
          effects: configuration.lineSeparator.reconfigure(
            EditorState.lineSeparator.of(separator),
          ),
        }),
      });
    }, [value, path]);
    useEffect(() => {
      view.current?.dispatch({
        effects: configuration.readonly.reconfigure(
          EditorState.readOnly.of(readOnly),
        ),
      });
    }, [readOnly, configuration, path]);
    useEffect(() => {
      view.current?.dispatch({
        effects: configuration.wrap.reconfigure(
          wrap ? EditorView.lineWrapping : [],
        ),
      });
    }, [wrap, configuration, path]);
    useEffect(() => {
      let live = true;
      setLanguageError("");
      const load = async () => {
        if (language === "stata") return StreamLanguage.define(stataMode);
        if (language === "latex") return StreamLanguage.define(stex);
        if (language === "matlab") return StreamLanguage.define(octave);
        if (language === "plaintext") return [];
        const names: Record<string, string> = {
          bash: "Shell",
          cpp: "C++",
          ini: "TOML",
          xml: "HTML",
        };
        const found = languages.find(
          (l) =>
            l.name.toLowerCase() ===
            (names[language] ?? language).toLowerCase(),
        );
        return found ? await found.load() : [];
      };
      void load()
        .then((extension) => {
          if (live)
            view.current?.dispatch({
              effects: configuration.language.reconfigure(extension),
            });
        })
        .catch(() => {
          if (live)
            setLanguageError(
              "Language support could not load. Plain text remains available.",
            );
        });
      return () => {
        live = false;
      };
    }, [language, configuration, path]);
    useEffect(() => {
      const complete = (context: CompletionContext) => {
        const prefix = context.matchBefore(
          /\\(?:cite\w*|ref|eqref|autoref)\{[^}]*$/,
        );
        if (!prefix) return null;
        const options = texCompletions([
          context.state.doc.toString(),
          ...completionTexts,
        ]);
        return {
          from:
            prefix.from +
            Math.max(
              prefix.text.lastIndexOf("{"),
              prefix.text.lastIndexOf(","),
            ) +
            1,
          options,
        };
      };
      view.current?.dispatch({
        effects: configuration.completion.reconfigure(
          language === "latex" ? autocompletion({ override: [complete] }) : [],
        ),
      });
    }, [completionTexts, language, configuration, path]);
    useEffect(() => {
      if (props.reveal?.line) goToLine(props.reveal.line, { focus: false });
    }, [props.reveal]);
    return (
      <div className="file-source-editor">
        {languageError && <p role="status">{languageError}</p>}
        <div className="file-editor-host" ref={host} />
        <div className="file-statusbar">
          <select
            aria-label="Source language"
            value={language}
            onChange={(e) => setLanguage(e.target.value)}
          >
            {FILE_LANGUAGES.map((l) => (
              <option key={l.id} value={l.id}>
                {l.label}
              </option>
            ))}
          </select>
          <label>
            <input
              type="checkbox"
              checked={wrap}
              onChange={(e) => setWrap(e.target.checked)}
            />{" "}
            Wrap lines
          </label>
          {readOnly && <span>Read only</span>}
          <div className="file-statusbar-end">
            <span>
              Ln {cursor.line}, Col {cursor.column}
            </span>
            <input
              aria-label="Go to source line"
              type="number"
              min={1}
              placeholder="Go to line"
              className="file-line-input"
              onKeyDown={(e) => {
                if (e.key === "Enter") goToLine(Number(e.currentTarget.value));
              }}
            />
          </div>
        </div>
      </div>
    );
  },
);
export default SourceEditor;
