import type { Person, Session, Status } from "@quorum/core";
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

/**
 * Render the people list into `container` and toast joins/leaves.
 * The baseline is seeded when the session reports "connected" (first sync) and
 * dropped on any other status, so reconnects do not toast everyone.
 */
export function mountPeople(container: HTMLElement, session: Session): () => void {
  let editing = false;
  let latest: Person[] = session.people();
  let known: Person[] | null = null;

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

  const render = (people: Person[]) => {
    latest = people;
    if (editing) return;
    container.replaceChildren(
      ...people.map((p) => {
        const li = document.createElement("li");
        li.className = "person";
        const dot = document.createElement("span");
        dot.className = "dot";
        dot.style.background = p.color;
        dot.setAttribute("aria-hidden", "true");
        li.append(dot);
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
