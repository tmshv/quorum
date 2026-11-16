import { DEFAULT_LANGUAGE, LANGUAGES, type LanguageId, languageById, normalizeRoom, randomRoom } from "@quorum/core";
import { RefreshCw, icon, selectBox } from "./icons";

const LANG_KEY = "quorum:lang";

function loadLangPref(): LanguageId {
  try {
    const raw = window.localStorage.getItem(LANG_KEY);
    if (raw !== null && languageById(raw).id === raw) return raw as LanguageId;
  } catch {
    // storage blocked: default language
  }
  return DEFAULT_LANGUAGE;
}

function saveLangPref(id: LanguageId): void {
  try {
    window.localStorage.setItem(LANG_KEY, id);
  } catch {
    // storage blocked: not remembered
  }
}

export function mountHome(root: HTMLElement): void {
  const main = document.createElement("main");
  main.className = "home";

  const h1 = document.createElement("h1");
  h1.className = "wordmark";
  h1.textContent = "quorum";

  const lede = document.createElement("p");
  lede.className = "lede";
  lede.textContent = "A shared code pad for interviews. Open a room, send the link, type together.";

  const note = document.createElement("p");
  note.className = "note";
  note.textContent = "Rooms aren't stored. When the last person leaves, the code is gone.";

  const form = document.createElement("form");
  form.className = "open-form";

  const host = document.createElement("span");
  host.className = "host";
  host.textContent = location.host + "/";

  const input = document.createElement("input");
  input.className = "room-input";
  input.value = randomRoom();
  input.maxLength = 40;
  input.spellcheck = false;
  input.autocomplete = "off";
  input.autocapitalize = "off";
  input.setAttribute("aria-label", "Room name");

  const reroll = document.createElement("button");
  reroll.type = "button";
  reroll.className = "icon-btn";
  reroll.title = "Pick another name";
  reroll.setAttribute("aria-label", "Pick another room name");
  reroll.append(icon(RefreshCw, 18));
  // Keep focus in the input: a focus hop would flash the form's focus border.
  reroll.addEventListener("mousedown", (e) => e.preventDefault());
  reroll.addEventListener("click", () => {
    input.value = randomRoom();
    input.focus();
  });

  const lang = document.createElement("select");
  lang.setAttribute("aria-label", "Language");
  lang.title = "Starting language (anyone in the room can change it)";
  for (const l of LANGUAGES) lang.append(new Option(l.label, l.id));
  lang.value = loadLangPref();

  const go = document.createElement("button");
  go.type = "submit";
  go.className = "go";
  go.textContent = "Open room";

  form.append(host, input, reroll, selectBox(lang, "lang-select"), go);
  form.addEventListener("submit", (e) => {
    e.preventDefault();
    const room = normalizeRoom(input.value) || normalizeRoom(randomRoom());
    const id = languageById(lang.value).id;
    saveLangPref(id);
    location.assign("/" + room + "?lang=" + encodeURIComponent(id));
  });

  main.append(h1, lede, form, note);
  root.replaceChildren(main);
  input.focus();
  input.select();
}
