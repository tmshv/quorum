import { autocompletion, closeBrackets, closeBracketsKeymap, completionKeymap } from "@codemirror/autocomplete";
import { defaultKeymap } from "@codemirror/commands";
import {
  bracketMatching, defaultHighlightStyle, foldGutter, foldKeymap, indentOnInput, syntaxHighlighting,
} from "@codemirror/language";
import { lintKeymap } from "@codemirror/lint";
import { highlightSelectionMatches, searchKeymap } from "@codemirror/search";
import { Compartment, EditorState, type Extension, Prec } from "@codemirror/state";
import {
  EditorView, crosshairCursor, drawSelection, dropCursor, highlightActiveLine,
  highlightActiveLineGutter, highlightSpecialChars, keymap, lineNumbers, rectangularSelection,
} from "@codemirror/view";
import * as Y from "yjs";
import { yCollab, ySyncAnnotation, yUndoManagerKeymap } from "y-codemirror.next";
import { type LanguageId, languageById } from "./languages";
import type { Session } from "./session";
import { type ThemeId, themeById } from "./themes";

export interface EditorHandle {
  view: EditorView;
  setTheme(id: ThemeId): void;
  setLanguage(id: LanguageId): Promise<void>;
  destroy(): void;
}

/**
 * `basicSetup` without `history()`/`historyKeymap`: CodeMirror's history would
 * record (and undo) other people's edits. Undo goes through the Y.UndoManager.
 */
export const collabSetup = [
  lineNumbers(),
  highlightActiveLineGutter(),
  highlightSpecialChars(),
  foldGutter(),
  drawSelection(),
  dropCursor(),
  EditorState.allowMultipleSelections.of(true),
  indentOnInput(),
  syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
  bracketMatching(),
  closeBrackets(),
  autocompletion(),
  rectangularSelection(),
  crosshairCursor(),
  highlightActiveLine(),
  highlightSelectionMatches(),
  keymap.of([
    ...closeBracketsKeymap,
    ...defaultKeymap,
    ...searchKeymap,
    ...foldKeymap,
    ...completionKeymap,
    ...lintKeymap,
  ]),
];

/** Local edits may not grow the document past this; the server caps it at 1 MiB encoded. */
export const MAX_DOC_CHARS = 900_000;

/**
 * Rejects local transactions that would grow the document past `max`
 * characters and calls `onTooLarge`. Remote (y-sync) changes always pass,
 * as do edits that do not grow the document.
 */
export function sizeGuard(max: number, onTooLarge: () => void): Extension {
  return EditorState.transactionFilter.of((tr) => {
    if (!tr.docChanged || tr.annotation(ySyncAnnotation) !== undefined) return tr;
    const len = tr.newDoc.length;
    if (len <= max || len <= tr.startState.doc.length) return tr;
    onTooLarge();
    return [];
  });
}

const MONO = '"JetBrains Mono", ui-monospace, monospace';

const fontTheme = EditorView.theme({
  "&": { height: "100%" },
  ".cm-scroller": { fontFamily: MONO },
  ".cm-content, .cm-gutters": { fontFamily: MONO },
});

/**
 * Remote carets as always-visible name flags, with no hover effects.
 * y-codemirror's line selection swaps the line's 6px left padding for a 4px
 * margin, which nudged fully selected lines 2px left in everyone else's view;
 * keep CodeMirror's own `.cm-line` padding instead. The content gets top room
 * so a flag on the first line is not clipped.
 */
export const remoteCursorTheme = EditorView.theme({
  ".cm-content": { paddingTop: "20px" },
  ".cm-yLineSelection": { margin: "0", padding: "0 2px 0 6px" },
  ".cm-ySelectionCaret": { borderLeftWidth: "2px", borderRightWidth: "0", marginLeft: "-1px", marginRight: "-1px" },
  ".cm-ySelectionCaretDot": { display: "none" },
  ".cm-ySelectionInfo, .cm-ySelectionCaret:hover > .cm-ySelectionInfo": {
    top: "auto",
    bottom: "100%",
    left: "-2px",
    padding: "1px 6px",
    borderRadius: "4px 4px 4px 0",
    color: "#15131f",
    fontFamily: MONO,
    fontSize: "12px",
    fontWeight: "700",
    lineHeight: "16px",
    opacity: "1",
    transition: "none",
    pointerEvents: "none",
  },
});

export function createEditor(
  parent: HTMLElement,
  session: Session,
  opts: { theme: ThemeId; lang: LanguageId; onTooLarge?: () => void },
): EditorHandle {
  const theme = new Compartment();
  const language = new Compartment();
  const undoManager = new Y.UndoManager(session.text);

  const view = new EditorView({
    parent,
    doc: session.text.toString(),
    extensions: [
      collabSetup,
      sizeGuard(MAX_DOC_CHARS, opts.onTooLarge ?? (() => {})),
      yCollab(session.text, session.awareness, { undoManager }),
      Prec.highest(keymap.of(yUndoManagerKeymap)),
      fontTheme,
      remoteCursorTheme,
      theme.of(themeById(opts.theme).extension),
      language.of([]),
    ],
  });

  let token = 0;
  let destroyed = false;
  const setLanguage = async (id: LanguageId): Promise<void> => {
    const mine = ++token;
    const ext = await languageById(id).load();
    if (destroyed || mine !== token) return;
    view.dispatch({ effects: language.reconfigure(ext) });
  };
  void setLanguage(opts.lang);

  return {
    view,
    setTheme(id) {
      view.dispatch({ effects: theme.reconfigure(themeById(id).extension) });
    },
    setLanguage,
    destroy() {
      destroyed = true;
      undoManager.destroy();
      view.destroy();
    },
  };
}
