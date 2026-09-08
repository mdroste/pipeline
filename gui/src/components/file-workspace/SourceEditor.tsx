import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import { basicSetup } from "codemirror";
import { Compartment, EditorState, Text } from "@codemirror/state";
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
    if (stream.match(/\b(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?\b/i))
      return "number";
    if (
      stream.match(
        /\b(?:regress|reg|ivregress|xtreg|reghdfe|gen|generate|replace|egen|summarize|sum|use|save|merge|append|keep|drop|if|else|foreach|forvalues|while|local|global|scalar|matrix|program|end|return|ereturn|capture|quietly|noisily|preserve|restore|import|export|clear|set|do|include|exit|estimates|test|predict|margins|log|display|di|assert|sort|bysort|by|collapse|reshape|tsset|xtset)\b/,
      )
    )
      return "keyword";
    if (stream.match(/[A-Za-z_]\w*/)) return "variableName";
    stream.next();
    return null;
  },
};
export interface SourceEditorHandle {
  goToLine: (line: number) => void;
  focus: () => void;
}
interface Props {
  value: string;
  path: string;
  onChange?: (text: string) => void;
  readOnly?: boolean;
  label?: string;
  onSave?: () => void;
  line?: number;
  completionTexts?: string[];
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
    const [wrap, setWrap] = useState(false);
    const [languageError, setLanguageError] = useState("");
    const [configuration] = useState(() => ({
      language: new Compartment(),
      wrap: new Compartment(),
      readonly: new Compartment(),
      completion: new Compartment(),
      lineSeparator: new Compartment(),
    }));
    const goToLine = (n: number) => {
      const editor = view.current;
      if (!editor) return;
      const line = editor.state.doc.line(
        Math.max(1, Math.min(Math.floor(n) || 1, editor.state.doc.lines)),
      );
      editor.dispatch({
        selection: { anchor: line.from },
        effects: EditorView.scrollIntoView(line.from, { y: "center" }),
      });
      editor.focus();
    };
    useImperativeHandle(ref, () => ({
      goToLine,
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
              if (update.docChanged)
                current.current.onChange?.(update.state.sliceDoc());
              if (update.selectionSet) {
                const s = update.state.selection.main;
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
              "&": { height: "100%", fontSize: "13px" },
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
      if (editor && value !== editor.state.sliceDoc())
        editor.dispatch({
          changes: {
            from: 0,
            to: editor.state.doc.length,
            insert: Text.of(value.split(/\r\n|\r|\n/)),
          },
          effects: configuration.lineSeparator.reconfigure(
            EditorState.lineSeparator.of(
              value.match(/\r\n|\r|\n/)?.[0] ?? editor.state.lineBreak,
            ),
          ),
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
      if (props.line) goToLine(props.line);
    }, [props.line, path]);
    return (
      <div className="file-source-editor">
        <div className="file-toolbar">
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
          <span>{readOnly ? "Read only" : "⌘/Ctrl+S to save"}</span>
          <label>
            Line{" "}
            <input
              aria-label="Go to source line"
              type="number"
              min={1}
              className="file-line-input"
              onKeyDown={(e) => {
                if (e.key === "Enter") goToLine(Number(e.currentTarget.value));
              }}
            />
          </label>
        </div>
        {languageError && <p role="status">{languageError}</p>}
        <div className="file-editor-host" ref={host} />
      </div>
    );
  },
);
export default SourceEditor;
