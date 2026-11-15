import { ChevronDown, RefreshCw, type IconNode, Link, Users, createElement } from "lucide";

export { ChevronDown, RefreshCw, Link, Users };

/** A decorative Lucide icon; the surrounding control carries the label. */
export function icon(node: IconNode, size = 16): SVGElement {
  return createElement(node, { width: size, height: size, "aria-hidden": "true", focusable: "false" });
}
