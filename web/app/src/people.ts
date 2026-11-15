import type { Person, Session, Status } from "@quorum/core";
import { ChevronDown, Users, icon } from "./icons";
import { toast } from "./toast";

export function diffPeople(
  prev: readonly Person[],
  next: readonly Person[],
): { joined: Person[]; left: Person[] } {
  const before = new Set(prev.filter((p) => !p.self).map((p) => p.clientId));
  const after = new Set(next.filter((p) => !p.self).map((p) => p.clientId));
  return {
    joined: next.filter((p) => !p.self && !before.has(p.clientId)),
    left: prev.filter((p) => !p.self && !after.has(p.clientId)),
  };
}

/** Wide screens start with the panel open; phones start with it folded. */
const WIDE = "(min-width: 641px)";

/**
 * A floating panel over the editor listing everyone in the room with their
 * presence, folding to a small "people" chip. Also toasts joins/leaves: the
 * baseline is seeded when the session reports "connected" (first sync) and
 * dropped on any other status, so reconnects do not toast everyone.
 */
export function mountPeople(panel: HTMLElement, session: Session): () => void {
  let editing = false;
  let latest: Person[] = session.people();
  let known: Person[] | null = null;

  panel.className = "people-panel";
  panel.setAttribute("aria-label", "People in this room");

  const toggle = document.createElement("button");
  toggle.type = "button";
  toggle.className = "people-toggle";
  const count = document.createElement("span");
  count.className = "people-count";
  const stack = document.createElement("span");
  stack.className = "people-stack";
  stack.setAttribute("aria-hidden", "true");
  const chevron = icon(ChevronDown, 14);
  chevron.classList.add("chevron");
  toggle.append(icon(Users, 16), count, stack, chevron);

  const list = document.createElement("ul");
  list.className = "people";
  list.id = "people-list";
  toggle.setAttribute("aria-controls", list.id);

  const setOpen = (open: boolean) => {
    list.hidden = !open;
    panel.dataset.open = String(open);
    toggle.setAttribute("aria-expanded", String(open));
    toggle.title = open ? "Hide people" : "Show people";
  };
  setOpen(window.matchMedia(WIDE).matches);
  toggle.addEventListener("click", () => setOpen(panel.dataset.open !== "true"));

  panel.replaceChildren(toggle, list);

  const startRename = (nameEl: HTMLElement) => {
    editing = true;
    const input = document.createElement("input");
    input.className = "rename";
    input.value = session.identity.name;
    input.maxLength = 32;
    input.setAttribute("aria-label", "Your name");
    let done = false;
    const finish = (commit: boolean) => {
      if (done) return;
      done = true;
      editing = false;
      if (commit && input.value.trim() !== "") session.rename(input.value);
      render(session.people());
    };
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") finish(true);
      else if (e.key === "Escape") finish(false);
    });
    input.addEventListener("blur", () => finish(false));
    nameEl.replaceWith(input);
    input.focus();
    input.select();
  };

  const dot = (p: Person) => {
    const d = document.createElement("span");
    d.className = "dot";
    d.style.setProperty("--c", p.color);
    if (p.away) d.classList.add("away");
    d.setAttribute("aria-hidden", "true");
    return d;
  };

  const render = (people: Person[]) => {
    latest = people;
    count.textContent = String(people.length);
    toggle.setAttribute("aria-label", `${people.length} ${people.length === 1 ? "person" : "people"} in this room`);
    stack.replaceChildren(...people.slice(0, 5).map(dot));
    if (editing) return;
    list.replaceChildren(
      ...people.map((p) => {
        const li = document.createElement("li");
        li.className = "person";
        if (p.away) li.classList.add("is-away");
        li.append(dot(p));
        if (p.self) {
          const name = document.createElement("button");
          name.type = "button";
          name.className = "person-name self";
          name.title = "Click to rename";
          name.textContent = p.name;
          name.addEventListener("click", () => startRename(name));
          const you = document.createElement("span");
          you.className = "you";
          you.textContent = "(you)";
          li.append(name, you);
        } else {
          const name = document.createElement("span");
          name.className = "person-name";
          name.textContent = p.name;
          li.append(name);
        }
        const state = document.createElement("span");
        state.className = "presence";
        state.textContent = p.away ? "away" : "online";
        li.append(state);
        return li;
      }),
    );
  };

  const onPeople = (people: Person[]) => {
    if (known) {
      const { joined, left } = diffPeople(known, people);
      for (const p of joined) toast(`${p.name} joined`);
      for (const p of left) toast(`${p.name} left`);
      known = people;
    }
    render(people);
  };
  const onStatus = (status: Status) => {
    known = status === "connected" ? session.people() : null;
  };

  render(latest);
  const offPeople = session.onPeople(onPeople);
  const offStatus = session.onStatus(onStatus);
  return () => {
    offPeople();
    offStatus();
  };
}
