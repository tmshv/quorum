import { normalizeRoom, randomRoom } from "@quorum/core";

const REROLL_ICON =
  '<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 12a9 9 0 1 1-2.64-6.36"/><path d="M21 3v6h-6"/></svg>';

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
  reroll.innerHTML = REROLL_ICON;
  reroll.addEventListener("click", () => {
    input.value = randomRoom();
    input.focus();
  });

  const go = document.createElement("button");
  go.type = "submit";
  go.className = "go";
  go.textContent = "Open room";

  form.append(host, input, reroll, go);
  form.addEventListener("submit", (e) => {
    e.preventDefault();
    const room = normalizeRoom(input.value) || normalizeRoom(randomRoom());
    location.assign("/" + room);
  });

  main.append(h1, lede, form, note);
  root.replaceChildren(main);
  input.focus();
  input.select();
}
