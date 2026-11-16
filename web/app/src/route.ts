import { normalizeRoom } from "@quorum/core";

export type Route = { kind: "home" } | { kind: "room"; room: string; canonical: string };

export function parseRoute(pathname: string): Route {
  const room = normalizeRoom(pathname.replace(/^\/+|\/+$/g, ""));
  if (room === "") return { kind: "home" };
  return { kind: "room", room, canonical: "/" + room };
}

export function serverUrl(loc: Pick<Location, "protocol" | "host">): string {
  const scheme = loc.protocol === "https:" ? "wss" : "ws";
  return `${scheme}://${loc.host}/ws`;
}
