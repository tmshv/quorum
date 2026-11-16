const TOAST_MS = 3000;

let host: HTMLElement | null = null;

export function toast(text: string): void {
  if (!host) {
    host = document.createElement("div");
    host.className = "toasts";
    host.setAttribute("role", "status");
    host.setAttribute("aria-live", "polite");
    document.body.append(host);
  }
  const el = document.createElement("div");
  el.className = "toast";
  el.textContent = text;
  host.append(el);
  setTimeout(() => el.remove(), TOAST_MS);
}
