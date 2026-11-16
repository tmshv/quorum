import { ChevronDown, RefreshCw, type IconNode, Link, Users, createElement } from "lucide";

export { ChevronDown, RefreshCw, Link, Users };

/** A decorative Lucide icon; the surrounding control carries the label. */
export function icon(node: IconNode, size = 16): SVGElement {
  return createElement(node, { width: size, height: size, "aria-hidden": "true", focusable: "false" });
}

/**
 * Wraps a native select so its chevron is a Lucide icon centred on the
 * control, instead of the browser's own arrow (which sits off-centre).
 */
export function selectBox(select: HTMLSelectElement, className = ""): HTMLElement {
  const box = document.createElement("span");
  box.className = className ? `select ${className}` : "select";
  const chevron = icon(ChevronDown, 14);
  chevron.classList.add("select-chevron");
  box.append(select, chevron);
  return box;
}
