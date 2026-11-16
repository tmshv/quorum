set shell := ["bash", "-euo", "pipefail", "-c"]

binary := "quorum"
# Tag names may hold shell metacharacters; keep only what a version needs,
# since the value is spliced into recipe command lines.
version := `v="$(git describe --tags --always --dirty 2>/dev/null)" || v=dev; printf '%s' "${v:-dev}" | tr -cd 'A-Za-z0-9._+-'`
targets := "x86_64-unknown-linux-musl:linux_amd64 aarch64-unknown-linux-musl:linux_arm64 x86_64-apple-darwin:darwin_amd64 aarch64-apple-darwin:darwin_arm64"

# List recipes
default:
    @just --list

# Install and build the web app into web/app/dist
web:
    cd web && npm ci && npm run build

# Build the web app and run the server locally
run *args: web
    QUORUM_VERSION={{ version }} cargo run -- {{ args }}

# Vite dev server with hot reload (run `cargo run` alongside it)
dev:
    cd web && npm run dev

# Format check, lints and all tests, Rust and web
test:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cd web
    # npm workspace installs keep pruning platform-optional packages from the
    # lockfile, which breaks `npm ci` on the Linux CI runners.
    for pkg in node_modules/@rolldown/binding-linux-x64-gnu node_modules/lightningcss-linux-x64-gnu; do
        if ! jq -r '.packages|keys[]' package-lock.json | grep -xF "$pkg" > /dev/null; then
            echo "error: web/package-lock.json lacks $pkg" >&2
            echo "fix: delete web/node_modules and web/package-lock.json, then run 'npm install' in web/" >&2
            exit 1
        fi
    done
    npm ci
    npm test
    npm run typecheck

# Build the binary for this machine into bin/
build: web
    QUORUM_VERSION={{ version }} cargo build --release
    mkdir -p bin
    cp target/release/{{ binary }} bin/{{ binary }}

# Cross-compile release archives and checksums into dist/ (needs zig and cargo-zigbuild)
dist: clean web
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p dist
    for entry in {{ targets }}; do
        target="${entry%%:*}"; platform="${entry#*:}"
        name="{{ binary }}_{{ version }}_${platform}"
        QUORUM_VERSION={{ version }} cargo zigbuild --release --target "$target"
        mkdir -p "dist/$name"
        cp "target/$target/release/{{ binary }}" LICENSE README.md "dist/$name/"
        tar -C dist -czf "dist/$name.tar.gz" "$name"
        rm -rf "dist/$name"
    done
    cd dist && shasum -a 256 *.tar.gz > checksums.txt

# Remove build output
clean:
    rm -rf bin dist target web/app/dist

# Bump the version like `npm version`, tag it and push: just release patch|minor|major|X.Y.Z
release bump="patch":
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -n "$(git status --porcelain)" ]]; then
        echo "error: working tree is not clean" >&2
        exit 1
    fi
    last="$(git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null || echo v0.0.0)"
    IFS=. read -r major minor patch <<< "${last#v}"
    case "{{ bump }}" in
        major) next="$((major + 1)).0.0" ;;
        minor) next="$major.$((minor + 1)).0" ;;
        patch) next="$major.$minor.$((patch + 1))" ;;
        *)
            if [[ ! "{{ bump }}" =~ ^v?[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
                echo "usage: just release patch|minor|major|X.Y.Z" >&2
                exit 1
            fi
            next="{{ bump }}"; next="${next#v}" ;;
    esac
    tag="v$next"
    if git rev-parse -q --verify "refs/tags/$tag" > /dev/null; then
        echo "error: tag $tag already exists" >&2
        exit 1
    fi
    just test
    echo "$last -> $tag"
    git tag -a "$tag" -m "$tag"
    git push origin HEAD "$tag"
