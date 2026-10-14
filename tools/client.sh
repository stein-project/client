#!/usr/bin/env bash
#
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# Copyright (c) 2026 Breno Xavier
#
# Builds the Stein Client: the interface (ui/, React + Vite) and the executable (Rust + Tauri), into build/client:
#
#   SteinClient.exe    the launcher: what players download, and what updates itself on startup
#   update.json        the executable's version and sha256, per OS: what the launcher reads at
#                      releases/latest/download/update.json to know whether there is a new version (and where to
#                      download it). Never publish it as client.json: launchers up to 0.2.9 read that name and must
#                      stay on 0.2.9.
#
# Needs Node (npm) and Rust (cargo). The interface is embedded in the executable.
#
#   tools/client.sh
#   RELEASE_REPO=Stein-Project/client tools/client.sh    repository of the release (the "base" in update.json)
#
# "base" in update.json is the absolute URL of this version's release (https://github.com/<repo>/releases/download/
# v<version>); the launcher downloads the executable from there, and only over https.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
OUT=build/client
REPO="${RELEASE_REPO:-Stein-Project/client}"
VERSION="$(grep -m1 -oE '^version = "[^"]+"' Cargo.toml | grep -oE '[0-9][^"]*')"
echo "== Stein Client $VERSION"

(cd ui && npm ci --no-audit --no-fund && npm run build)
cargo build --release -p stein-client

rm -rf "$OUT"; mkdir -p "$OUT"
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) os=windows; ext=.exe ;;
  Darwin) os=osx; ext= ;;
  *) os=linux; ext= ;;
esac
cp "target/release/SteinClient$ext" "$OUT/SteinClient$ext"
sha() { sha256sum "$1" | cut -d' ' -f1; }
cat > "$OUT/update.json" <<JSON
{
  "version": "$VERSION",
  "base": "https://github.com/$REPO/releases/download/v$VERSION",
  "files": {
    "$os": {
      "SteinClient$ext": "$(sha "$OUT/SteinClient$ext")"
    }
  }
}
JSON
cat "$OUT/update.json"
ls -la "$OUT"
