# quorum

A shared code editor. Open a room, send the link, and everyone with it edits
the same text in real time.

Nothing is stored on disk. The server keeps a room's document in memory while
someone is in the room and relays changes between the people in it. When the
last person leaves, the room and its code are gone.

The whole thing ships as one Rust binary with the web page embedded.

## Features

- **Rooms by URL.** Every single-segment path is a room:
  `https://your.host/kernel-panic-42`. The front page suggests a random name.
- **Shared editor.** [CodeMirror 6](https://codemirror.net) bound to a
  [Yjs](https://yjs.dev) document, so concurrent edits merge without
  conflicts.
- **Live cursors and names.** See who is where: every cursor carries a name
  flag, and selections show in the person's color. A floating panel lists
  everyone as online or away (tab in the background or idle for two minutes).
  Names are editable and kept per tab, never written to `localStorage`.
- **Languages.** Syntax highlighting for JavaScript, TypeScript, Python, Go,
  Rust, Java, C, C++, SQL, JSON, GeoJSON, CSS, HTML, Swift, Kotlin,
  Objective-C, Bash, Protobuf and plain text. Pick the starting language
  when you open a room; after that it is shared, and anyone can change it for
  everyone.
- **Two themes.** Cool Glow (dark, default) and Rosé Pine Dawn (light), both
  from [thememirror](https://github.com/vadimdemedes/thememirror). Your choice
  is remembered in `localStorage`.
- **Copy and download.** Copy the code or save it with a matching extension.
- **Link previews.** Shared room links unfurl in chats and social networks
  with a card showing the room's name, drawn by the server (`/og/<room>.png`).
  Room pages carry `noindex`; `robots.txt` admits only link-preview bots.
- **No accounts, no code execution.**

## How it works

```
browser ──ws──▶ reverse proxy ──▶ quorum (yrs) ──▶ other browsers in the room
```

- The server speaks the [y-websocket](https://github.com/yjs/y-websocket)
  protocol over `/ws/<room>`, using [yrs](https://github.com/y-crdt/y-crdt).
  Each room holds one document and one awareness state (cursors, names).
- A room is created by its first connection and dropped, with its document,
  when the last connection closes. A server restart empties every room.
- The client reconnects on its own. A browser that is still open after a
  restart pushes its copy back into the new empty room, so a quick restart
  loses little. Do not rely on this.
- The server pings every 30 s and closes connections that stay silent for 90 s.

### Limits

All server limits are hard caps in [`src/limits.rs`](src/limits.rs), so the
server's memory use is bounded. The editor's length limit is a guard in the
web client.

| Limit                         | Value                             |
| ----------------------------- | --------------------------------- |
| Connections in total          | 1000                              |
| Connections per IP            | 8                                 |
| Rooms                         | 200                               |
| People per room               | 16                                |
| Incoming message              | 2 MiB                             |
| Document per room             | 1 MiB (encoded state)             |
| Paste or typing in the editor | about 900,000 characters          |
| Awareness state               | 4 KiB                             |
| Awareness client IDs per conn | 8                                 |
| Removed client IDs remembered | 1024 per room                     |
| Message rate per connection   | 120/s, burst 360                  |
| Refused messages before drop  | 100 within a strike window        |
| Strike window                 | ends after 10 s without a refusal |
| Outgoing queue per connection | 512 messages                      |
| Send timeout                  | 10 s                              |
| Ping interval                 | 30 s                              |
| Idle timeout                  | 90 s                              |
| Name length                   | 32 characters                     |
| Room name                     | `[a-z0-9][a-z0-9-]{0,39}`         |

The message rate is a token bucket per connection. A document update (a Yjs
sync message) is never dropped, because later updates depend on it: an update
over the budget closes the connection with 4004. A cursor or name update over
the budget is dropped and counts as a refusal, since the sender's next state
replaces it anyway. Cursor and name updates that change nothing (echoes of
states the server already has) cost nothing. The editor refuses local edits
that would grow the code past about 900,000 characters and says so, instead of
sending a change the server would reject.

When a connection is refused or dropped, the server closes the WebSocket with
one of these codes:

| Code | Name          | Meaning                                       | Client retries |
| ---- | ------------- | --------------------------------------------- | -------------- |
| 4000 | BAD_ROOM      | room name fails the regex                     | no             |
| 4001 | ROOM_FULL     | 16 people already in the room                 | on user click  |
| 4002 | BUSY          | total, per-IP or room-count cap reached       | on user click  |
| 4003 | DOC_TOO_LARGE | doc would pass 1 MiB, or message over 2 MiB   | no (reload)    |
| 4004 | RATE_LIMITED  | update over budget, or more than 100 refusals | on user click  |
| 4005 | TOO_SLOW      | outbox full, reader cannot keep up            | yes (auto)     |
| 4006 | PROTOCOL      | undecodable message or oversized awareness    | no             |

## Running

Download an archive for your platform from
[Releases](https://github.com/tmshv/quorum/releases), or build from source
(the web app must be built first, because the binary embeds it):

```sh
git clone https://github.com/tmshv/quorum && cd quorum
just build
./bin/quorum --listen 127.0.0.1:20481
```

To install into Cargo's bin directory, build the web app in a checkout first:

```sh
git clone https://github.com/tmshv/quorum && cd quorum
just web
cargo install --path .
```

`cargo install --git` does not work: a fresh clone has no `web/app/dist`, so
the binary it builds answers every page with 503.

Flags:

| Flag        | Default           | Meaning                    |
| ----------- | ----------------- | -------------------------- |
| `--listen`  | `127.0.0.1:20481` | Address to serve HTTP on   |
| `--version` |                   | Print the version and exit |

### Behind a reverse proxy

quorum is meant to sit behind a reverse proxy that terminates TLS. It takes
the visitor's address from the first `X-Forwarded-For` entry, so the proxy
**must overwrite** that header rather than append to it. Otherwise clients can
spoof it and get around the per-IP cap. Never expose quorum directly to the
internet. The WebSocket origin must match the `Host` the proxy forwards.
Link previews build absolute URLs from `Host` and `X-Forwarded-Proto`, so the
proxy should pass both (Caddy does by default).

A [Caddy](https://caddyserver.com) site is enough:

```caddyfile
code.example.com {
	reverse_proxy 127.0.0.1:20481 {
		header_up X-Forwarded-For {remote_host}
	}
}
```

Since the server stores nothing, it can run with no user, no writable
filesystem and loopback-only networking (for example systemd's
`DynamicUser=yes`, `ProtectSystem=strict`, `IPAddressAllow=localhost`).

## Development

Requires Rust, Node (see [`.node-version`](.node-version)),
[just](https://github.com/casey/just) and [jq](https://jqlang.org).

```sh
just run          # build the web app, serve on 127.0.0.1:20481
just dev          # Vite dev server with hot reload (run `cargo run` alongside)
just test         # rustfmt, clippy, cargo test, web tests and typecheck
just build        # bin/quorum for this machine
just dist         # cross-compiled archives and checksums in dist/
```

`just dist` needs [zig](https://ziglang.org) and
[cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild).

The web app lives in [`web/`](web), an npm workspace with a framework-free
`core` package and the Vite `app`. It is embedded into the binary at build
time. The version comes from the `QUORUM_VERSION` environment variable at build
time (the justfile sets it from `git describe`), else `dev`.

`just test` also checks that `web/package-lock.json` still lists the Linux
optional packages that CI needs. npm workspace installs on other platforms
tend to prune them. If the check fails, delete `web/node_modules` and
`web/package-lock.json`, then run `npm install` in `web/`.

### Releasing

Releases are driven by tags, the way `npm version` works:

```sh
just release patch   # v1.2.3 -> v1.2.4
just release minor   # v1.2.3 -> v1.3.0
just release major   # v1.2.3 -> v2.0.0
just release 2.0.0   # an explicit version
```

The recipe checks that the working tree is clean, runs the tests, creates an
annotated `vX.Y.Z` tag and pushes the current branch and the tag. The
[release workflow](.github/workflows/release.yml) then tests and builds
binaries for Linux and macOS, amd64 and arm64. The Linux builds are static
(musl); the macOS builds link the system library. It publishes a
GitHub release with the list of changes since the previous tag, download links
and SHA-256 checksums.

[CI](.github/workflows/ci.yml) runs formatting checks, lints, tests and a
build on every pull request and on pushes to the main branch.

## License

[MIT](LICENSE). Bundled third-party code keeps its own licenses:
[thememirror](https://github.com/vadimdemedes/thememirror) (MIT) and
[JetBrains Mono](https://github.com/JetBrains/JetBrainsMono), shipped through
`@fontsource/jetbrains-mono` and, for the preview cards, as TTF in
[`assets/fonts`](assets/fonts) (SIL OFL 1.1, [text](assets/fonts/OFL.txt)).
