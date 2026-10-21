#!/usr/bin/env bash
#
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# Copyright (c) 2026 Breno Xavier
#
# Helpers to read and write the Stein registry (Stein-Project/registry), sourced by the release workflows
# (". .github/scripts/registry.sh"). The registry holds what the Stein Loader, the Stein Client and the installer look
# up at run time; a release writes its part there (the installer: "latest" in stein.json; a mod: mods/<id>.json), and
# the registry's own workflow signs and publishes it.
#
# Writing needs REGISTRY_TOKEN: a fine-grained token with "Contents: write" on the registry only. Each write is one
# commit through the contents API, under the project owner's name and noreply address, and none when the content is
# already there. With ENSAIO=true (the dry run) nothing is written: the new content is printed.
#
# Several repositories write here, and two releases can do it at the same moment. The contents API refuses a commit
# made on top of a branch that moved (409), and a file written from a copy read earlier could undo someone else's
# change (stein.json is also edited by hand), so registry_update reads the file, builds the new content from it and
# writes it only over the exact version it read, starting again from the read when anything moved in between.
#
# The same file is kept in the installer, the Stein Client and every official mod repository
# (.github/scripts/registry.sh): change all of them together.

REGISTRY_REPO="${REGISTRY_REPO:-Stein-Project/registry}"
# Where the registry's workflow publishes it, signed: what the programs read and what an index's "source" points to.
REGISTRY_URL="${REGISTRY_URL:-https://stein-project.github.io/registry/}"

# _registry_read <path> <out file>: writes the file to <out file> and prints its blob sha; prints nothing and leaves
# <out file> empty when the file does not exist yet; fails when the registry cannot be read.
_registry_read() {
  local path="$1" out="$2" token="${REGISTRY_TOKEN:-${GH_TOKEN:-}}" body err sha
  err="$(mktemp)"
  if ! body="$(GH_TOKEN="$token" gh api "repos/$REGISTRY_REPO/contents/$path" 2> "$err")"; then
    if grep -q 'HTTP 404' "$err"; then
      rm -f "$err"; : > "$out"; return 0
    fi
    cat "$err" >&2; rm -f "$err"; return 1
  fi
  rm -f "$err"
  command jq -r .content <<< "$body" | tr -d '\r' | base64 -d > "$out" || return 1
  sha="$(command jq -r '.sha // empty' <<< "$body" | tr -d '\r')" || return 1
  [ -n "$sha" ] || return 1
  echo "$sha"
}

# registry_update <path> <commit message> <build function>
#   The build function is called as "<function> <current file or ""> <out file>": it writes the new content to <out
#   file>, or leaves it missing to write nothing. It runs again on every attempt, always on the version just read.
registry_update() {
  local path="$1" msg="$2" build="$3" work try sha base
  work="$(mktemp -d)"
  for try in 1 2 3 4 5 6; do
    if [ "$try" -gt 1 ]; then
      echo "registry $path: attempt $try (the registry moved or did not answer)"
      sleep $((try * try * 2))
    fi
    rm -f "$work/current.json" "$work/new.json"
    sha="$(_registry_read "$path" "$work/current.json")" || continue
    base=""; [ -n "$sha" ] && base="$work/current.json"
    "$build" "$base" "$work/new.json" || { rm -rf "$work"; return 1; }
    if [ ! -f "$work/new.json" ]; then
      rm -rf "$work"; return 0
    fi
    command jq -e . "$work/new.json" > /dev/null \
      || { echo "::error::the new $path is not valid JSON"; rm -rf "$work"; return 1; }
    if [ "${ENSAIO:-false}" = true ]; then
      echo "registry $REGISTRY_REPO/$path would become (dry run, nothing written):"
      cat "$work/new.json"
      rm -rf "$work"; return 0
    fi
    [ -n "${REGISTRY_TOKEN:-}" ] || {
      echo "::error::the REGISTRY_TOKEN secret is missing (Contents: write on $REGISTRY_REPO)"
      rm -rf "$work"; return 1
    }
    if [ -n "$sha" ] \
        && [ "$(command jq -S . "$work/current.json" | tr -d '\r')" = "$(command jq -S . "$work/new.json" | tr -d '\r')" ]; then
      echo "registry $path already up to date"
      rm -rf "$work"; return 0
    fi
    local args=(-X PUT "repos/$REGISTRY_REPO/contents/$path"
      -f message="$msg"
      -f content="$(base64 < "$work/new.json" | tr -d '\r\n')"
      -f "committer[name]=Breno Xavier" -f "committer[email]=41960989+brenoxavier@users.noreply.github.com"
      -f "author[name]=Breno Xavier" -f "author[email]=41960989+brenoxavier@users.noreply.github.com")
    # With the sha the write only goes over the version just read; without it, only when the file still does not exist.
    [ -n "$sha" ] && args+=(-f sha="$sha")
    if GH_TOKEN="$REGISTRY_TOKEN" gh api "${args[@]}" > /dev/null; then
      echo "registry $path updated"
      rm -rf "$work"; return 0
    fi
  done
  rm -rf "$work"
  echo "::error::could not write $path to $REGISTRY_REPO"
  return 1
}
