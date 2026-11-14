import type { Person, Session } from "@quorum/core";
import { toast } from "./toast";

/**
 * Render the people list into `container` and toast joins/leaves.
 * Toasts are skipped for self and for the first snapshot.
 */
export function mountPeople(container: HTMLElement, session: Session): () => void {
  let editing = false;
  let latest: Person[] = session.people();
  let known: Map<number, string> | null = null;

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
    const next = new Map<number, string>();
    for (const p of people) if (!p.self) next.set(p.clientId, p.name);
    if (known) {
      for (const [id, name] of next) if (!known.has(id)) toast(`${name} joined`);
      for (const [id, name] of known) if (!next.has(id)) toast(`${name} left`);
    }
    known = next;
    render(people);
  };

  render(latest);
  return session.onPeople(onPeople);
}
