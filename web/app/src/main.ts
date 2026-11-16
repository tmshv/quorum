import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/700.css";
import "./styles.css";
import { THEMES, loadThemePref, themeById } from "@quorum/core";
import { applyChrome } from "./chrome";
import { mountHome } from "./home";
import { mountRoom } from "./room";
import { parseRoute } from "./route";

const root = document.getElementById("app")!;
const route = parseRoute(location.pathname);

if (route.kind === "room") {
  if (location.pathname !== route.canonical) history.replaceState(null, "", route.canonical + location.search + location.hash);
  mountRoom(root, route.room);
} else {
  let pref = THEMES[0]!.id;
  try {
    pref = loadThemePref(window.localStorage);
  } catch {
    // storage blocked: default theme
  }
  applyChrome(themeById(pref), document.documentElement);
  mountHome(root);
}
