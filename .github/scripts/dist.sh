#!/usr/bin/env bash
#
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# Copyright (c) 2026 Breno Xavier
#
# Helpers of the release workflows, sourced (". .github/scripts/dist.sh"). They write to the organisation's public
# distribution repository (Stein-Project/stein-*) with the token in GH_TOKEN (secret DIST_TOKEN).
#
# The same file is kept in the installer, client and sdk repositories (.github/scripts/dist.sh): change all three
# together.

# jq on the Windows runner ends lines with CRLF, and a \r at the end of a value breaks comparisons and paths.
jq() { command jq "$@" | tr -d '\r'; }

dist_need_token() {  # <secret name>
  [ -n "${GH_TOKEN:-}" ] || { echo "::error::secret $1 is not set in this repository (Settings > Secrets and variables > Actions)"; return 1; }
}

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

# dist_hub <repo> <commit message> <jq filter> [jq options...]
# Rewrites stein.json on the repository's default branch through the contents API, with the sha it read: if someone
# changed the file in between, the write fails instead of erasing their change. Only what the filter touches changes
# (the mods index, discord and news stay as they are).
dist_hub() {
  local repo="$1" msg="$2" filter="$3" cur sha json new
  shift 3
  cur="$(gh api "repos/$repo/contents/stein.json")" || { echo "::error::cannot read stein.json of $repo"; return 1; }
  sha="$(jq -r .sha <<< "$cur")"
  json="$(jq -r .content <<< "$cur" | tr -d '\n' | base64 -d)"
  new="$(jq "$@" "$filter" <<< "$json")" || { echo "::error::jq failed on stein.json of $repo"; return 1; }
  if [ "$new" = "$json" ]; then
    echo "stein.json of $repo was already up to date"
    return 0
  fi
  gh api -X PUT "repos/$repo/contents/stein.json" -f message="$msg" -f sha="$sha" \
    -f content="$(printf '%s\n' "$new" | base64 -w0)" >/dev/null
  echo "stein.json of $repo updated"
}

# dist_push_dir <repo> <branch> <dir with the new content> <commit message> <author name> <author email>
# Replaces everything on <branch> (except dotfiles: .git, .gitattributes) with <dir> and pushes one commit, or nothing when it is
# already the same. Used for the pages of the distribution repositories (README, guide, examples).
dist_push_dir() {
  local repo="$1" branch="$2" src="$3" msg="$4" name="$5" email="$6" work
  work="$(mktemp -d)"
  local url="https://x-access-token:${GH_TOKEN}@github.com/$repo.git"
  git clone -q --branch "$branch" --single-branch "$url" "$work/repo" 2>/dev/null || git init -q -b "$branch" "$work/repo"
  find "$work/repo" -mindepth 1 -maxdepth 1 ! -name '.*' -exec rm -rf {} +
  cp -R "$src"/. "$work/repo/"
  (
    cd "$work/repo" || exit 1
    git add -A
    if git diff --cached --quiet; then
      echo "$repo ($branch) was already up to date"
    else
      git -c user.name="$name" -c user.email="$email" commit -q -m "$msg"
      git push -q "$url" "HEAD:$branch"
      echo "$repo ($branch) updated"
    fi
  )
  rm -rf "$work"
}
