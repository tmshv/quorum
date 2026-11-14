export interface MenuItem {
  label: string;
  run: () => void;
}

/** A small button that opens a dropdown of actions. */
export function createMenu(label: string, items: readonly MenuItem[]): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "menu";

  const button = document.createElement("button");
  button.type = "button";
  button.className = "btn";
  button.textContent = label;
  button.setAttribute("aria-haspopup", "menu");
  button.setAttribute("aria-expanded", "false");

  const list = document.createElement("div");
  list.className = "menu-list";
  list.setAttribute("role", "menu");
  list.hidden = true;

  const close = () => {
    list.hidden = true;
    button.setAttribute("aria-expanded", "false");
  };
  const open = () => {
    list.hidden = false;
    button.setAttribute("aria-expanded", "true");
  };

  for (const item of items) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "menu-item";
    b.setAttribute("role", "menuitem");
    b.textContent = item.label;
    b.addEventListener("click", () => {
      close();
      item.run();
    });
    list.append(b);
  }

  button.addEventListener("click", () => (list.hidden ? open() : close()));
  document.addEventListener("click", (e) => {
    if (!wrap.contains(e.target as Node)) close();
  });
  wrap.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      close();
      button.focus();
    }
  });

  wrap.append(button, list);
  return wrap;
}
