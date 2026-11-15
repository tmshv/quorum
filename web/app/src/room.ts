import {
  DEFAULT_LANGUAGE,
  LANGUAGES,
  THEMES,
  type LanguageId,
  type Status,
  createEditor,
  createSession,
  languageById,
  loadIdentity,
  loadThemePref,
  saveThemePref,
  themeById,
} from "@quorum/core";
import { applyChrome } from "./chrome";
import { Link, icon, selectBox } from "./icons";
import { createMenu } from "./menu";
import { mountPeople } from "./people";
import { trackPresence } from "./presence";
import { serverUrl } from "./route";
import { toast } from "./toast";

const STATUS_TEXT: Record<Status, string> = {
  connecting: "Connecting…",
  connected: "",
  reconnecting: "Reconnecting…",
  "room-full": "Room is full",
  busy: "Server is busy",
  "rate-limited": "Rate limited",
  "bad-room": "Invalid room name",
  "too-large": "Document too large — reload to start over",
  error: "Connection error — reload to try again",
};
const RETRYABLE: ReadonlySet<Status> = new Set(["room-full", "busy", "rate-limited"]);

function storage(kind: "local" | "session"): Storage | null {
  try {
    return kind === "local" ? window.localStorage : window.sessionStorage;
  } catch {
    return null;
  }
}

async function copy(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    toast("Copied");
  } catch {
    toast("Copy failed");
  }
}

function download(filename: string, text: string): void {
  const url = URL.createObjectURL(new Blob([text], { type: "text/plain;charset=utf-8" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

function labelled(text: string, control: HTMLElement): HTMLElement {
  const label = document.createElement("label");
  label.className = "field";
  const span = document.createElement("span");
  span.className = "field-label";
  span.textContent = text;
  label.append(span, control);
  return label;
}

export function mountRoom(root: HTMLElement, room: string): void {
  const themePref = loadThemePref(storage("local"));
  const initialLang = takeInitialLang(room);
  applyChrome(themeById(themePref), document.documentElement);

  const session = createSession({
    room,
    serverUrl: serverUrl(location),
    identity: loadIdentity(room, storage("session")),
    storage: storage("session"),
    initialLang,
  });

  const shell = document.createElement("div");
  shell.className = "room";

  const bar = document.createElement("header");
  bar.className = "bar";

  const title = document.createElement("a");
  title.className = "title";
  title.href = "/";
  title.append("quorum", Object.assign(document.createElement("span"), { className: "sep", textContent: " / " }));
  title.append(Object.assign(document.createElement("span"), { className: "room-name", textContent: room }));

  const copyLink = document.createElement("button");
  copyLink.type = "button";
  copyLink.className = "icon-btn small";
  copyLink.title = "Copy link";
  copyLink.setAttribute("aria-label", "Copy room link");
  copyLink.append(icon(Link, 16));
  copyLink.addEventListener("click", () => void copy(location.origin + "/" + room));

  let lang: LanguageId = initialLang ?? DEFAULT_LANGUAGE;
  const initial = session.meta.get("lang");
  if (typeof initial === "string") lang = languageById(initial).id;

  const langSelect = document.createElement("select");
  for (const l of LANGUAGES) langSelect.append(new Option(l.label, l.id));
  langSelect.value = lang;
  langSelect.addEventListener("change", () => session.setLang(langSelect.value as LanguageId));

  const themeSelect = document.createElement("select");
  for (const t of THEMES) themeSelect.append(new Option(t.label, t.id));
  themeSelect.value = themePref;

  const menu = createMenu("Code", [
    { label: "Copy code", run: () => void copy(session.text.toString()) },
    {
      label: "Download code",
      run: () => download(`${room}.${languageById(lang).ext}`, session.text.toString()),
    },
  ]);

  const pill = document.createElement("span");
  pill.className = "pill";
  pill.hidden = true;
  const pillText = document.createElement("span");
  const retry = document.createElement("button");
  retry.type = "button";
  retry.className = "btn small";
  retry.textContent = "Retry";
  retry.hidden = true;
  retry.addEventListener("click", () => session.retry());
  pill.append(pillText, retry);

  const controls = document.createElement("div");
  controls.className = "controls";
  controls.append(labelled("lang", selectBox(langSelect)), labelled("theme", selectBox(themeSelect)), menu, pill);

  bar.append(title, copyLink, controls);

  const editorHost = document.createElement("main");
  editorHost.className = "editor";

  const people = document.createElement("aside");
  const stage = document.createElement("div");
  stage.className = "stage";
  stage.append(editorHost, people);

  shell.append(bar, stage);
  root.replaceChildren(shell);

  const editor = createEditor(editorHost, session, {
    theme: themePref,
    lang,
    onTooLarge: () => toast("Too large: the room's code is limited to about 1 MB"),
  });

  themeSelect.addEventListener("change", () => {
    const def = themeById(themeSelect.value);
    saveThemePref(def.id, storage("local"));
    applyChrome(def, document.documentElement);
    editor.setTheme(def.id);
  });

  session.onLang((id) => {
    lang = languageById(id).id;
    langSelect.value = lang;
    void editor.setLanguage(lang);
  });

  const showStatus = (s: Status) => {
    pillText.textContent = STATUS_TEXT[s];
    pill.hidden = s === "connected";
    pill.dataset.status = s;
    retry.hidden = !RETRYABLE.has(s);
  };
  showStatus("connecting");
  session.onStatus(showStatus);

  mountPeople(people, session);
  const stopPresence = trackPresence((away) => session.setAway(away));

  window.addEventListener("pageshow", (e) => {
    if (e.persisted) location.reload();
  });
  window.addEventListener("pagehide", () => {
    stopPresence();
    editor.destroy();
    session.destroy();
  });
}

/**
 * The language picked on the home page arrives as `?lang=`; it only seeds a
 * brand-new room. Drop it from the address bar so shared links stay clean.
 */
function takeInitialLang(room: string): LanguageId | undefined {
  const raw = new URLSearchParams(location.search).get("lang");
  if (raw === null) return undefined;
  history.replaceState(null, "", "/" + room + location.hash);
  const def = languageById(raw);
  return def.id === raw ? def.id : undefined;
}
