#!/usr/bin/env bash
#
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# Copyright (c) 2026 Breno Xavier
#
# Helpers of the release workflows, sourced (". .github/scripts/dist.sh"). Every release is published in the
# repository of its own source, with the workflow's token (GH_TOKEN: github.token, "contents: write").
#
# The same file is kept in the installer, client and sdk repositories (.github/scripts/dist.sh): change all three
# together.

# jq on the Windows runner ends lines with CRLF, and a \r at the end of a value breaks comparisons and paths.
jq() { command jq "$@" | tr -d '\r'; }

# dist_release <repo> <tag> <title> <notes file> <prerelease 0|1> <files...>
# Idempotent: a published release of that tag is kept as it is (running again publishes nothing twice), and a draft
# left by an attempt that fell halfway (gh creates the draft, uploads, then publishes) is removed first.
dist_release() {
  local repo="$1" tag="$2" title="$3" notes="$4" pre="$5" draft
  shift 5
  draft="$(gh release view "$tag" --repo "$repo" --json isDraft -q .isDraft 2>/dev/null || true)"
  if [ "$draft" = false ]; then
    echo "$repo $tag is already published: kept as it is"
    return 0
  fi
  if [ "$draft" = true ]; then
    gh release delete "$tag" --repo "$repo" --yes
    echo "draft $tag of an earlier attempt removed"
  fi
  local extra=()
  [ "$pre" = 1 ] && extra+=(--prerelease)
  gh release create "$tag" "$@" --repo "$repo" --title "$title" --notes-file "$notes" "${extra[@]}"
  echo "release: https://github.com/$repo/releases/tag/$tag"
}
