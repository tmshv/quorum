import { Compartment } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { basicSetup } from "codemirror";
import * as Y from "yjs";
import { yCollab } from "y-codemirror.next";
import { type LanguageId, languageById } from "./languages";
import type { Session } from "./session";
import { type ThemeId, themeById } from "./themes";

export interface EditorHandle {
  view: EditorView;
  setTheme(id: ThemeId): void;
  setLanguage(id: LanguageId): Promise<void>;
  destroy(): void;
}

const fontTheme = EditorView.theme({
  "&": { height: "100%" },
  ".cm-scroller": { fontFamily: '"JetBrains Mono", ui-monospace, monospace' },
  ".cm-content, .cm-gutters": { fontFamily: '"JetBrains Mono", ui-monospace, monospace' },
});

export function createEditor(
  parent: HTMLElement,
  session: Session,
  opts: { theme: ThemeId; lang: LanguageId },
): EditorHandle {
  const theme = new Compartment();
  const language = new Compartment();
  const undoManager = new Y.UndoManager(session.text);

  const view = new EditorView({
    parent,
    doc: session.text.toString(),
    extensions: [
      basicSetup,
      yCollab(session.text, session.awareness, { undoManager }),
      fontTheme,
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
