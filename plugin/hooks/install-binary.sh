#!/bin/bash
set -euo pipefail

PLUGIN_ROOT="${CLAUDE_PLUGIN_ROOT:?CLAUDE_PLUGIN_ROOT must be set}"
VERSION=$(awk -F'"' '/"version":/ {print $4; exit}' "$PLUGIN_ROOT/.claude-plugin/plugin.json")
BIN="$PLUGIN_ROOT/bin/cosmic-grackle"
STAMP="$PLUGIN_ROOT/bin/.installed-version"

if [[ -x "$BIN" && -f "$STAMP" && "$(cat "$STAMP" 2>/dev/null)" == "$VERSION" ]]; then
  exit 0
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "cosmic-grackle binary is macOS-only; skipping install on $(uname -s)" >&2
  exit 0
fi

if [[ "$VERSION" == "0.0.0-dev" ]]; then
  echo "cosmic-grackle plugin.json reports version 0.0.0-dev; skipping binary fetch (build locally with 'cargo build --release' and copy into bin/)" >&2
  exit 0
fi

URL="https://github.com/ngerakines/cosmic-grackle/releases/download/${VERSION}/cosmic-grackle-plugin.tar.gz"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "cosmic-grackle: fetching binary v${VERSION} from GitHub release..." >&2
if ! curl -fsSL "$URL" -o "$TMP/plugin.tar.gz"; then
  echo "cosmic-grackle: failed to download $URL" >&2
  exit 1
fi

tar -xzf "$TMP/plugin.tar.gz" -C "$TMP" cosmic-grackle/bin/cosmic-grackle
mkdir -p "$PLUGIN_ROOT/bin"
mv "$TMP/cosmic-grackle/bin/cosmic-grackle" "$BIN"
chmod +x "$BIN"
xattr -d com.apple.quarantine "$BIN" 2>/dev/null || true
echo "$VERSION" > "$STAMP"
echo "cosmic-grackle: installed v${VERSION}" >&2
