# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# Copyright (c) 2026 Breno Xavier
#
# Writes a file into the Stein registry (Stein-Project/registry), whose own workflow signs and publishes it.
#
#   . .github/scripts/registry.sh
#   registry_put <path in the registry> <local file> <commit message>
#
# GH_TOKEN must be able to write to the registry (the REGISTRY_TOKEN secret: a fine-grained token with Contents:
# write on it). Nothing is committed when the registry already holds the same bytes, so running a release again
# writes nothing twice. The commit carries the project's no-reply identity, never the token owner's address.
REGISTRY_REPO="${REGISTRY_REPO:-Stein-Project/registry}"

registry_put() {
    local path="$1" file="$2" msg="$3"
    local current sha
    current="$(gh api "repos/$REGISTRY_REPO/contents/$path" -H "Accept: application/vnd.github.raw" 2>/dev/null || true)"
    if [ -n "$current" ] && [ "$current" == "$(cat "$file")" ]; then
        echo "registry: $path unchanged"
        return 0
    fi
    sha="$(gh api "repos/$REGISTRY_REPO/contents/$path" --jq .sha 2>/dev/null || true)"
    gh api -X PUT "repos/$REGISTRY_REPO/contents/$path" \
        -f message="$msg" \
        -f content="$(base64 -w0 < "$file")" \
        -f "committer[name]=Breno Xavier" \
        -f "committer[email]=41960989+brenoxavier@users.noreply.github.com" \
        -f "author[name]=Breno Xavier" \
        -f "author[email]=41960989+brenoxavier@users.noreply.github.com" \
        ${sha:+-f sha="$sha"} > /dev/null
    echo "registry: $path written"
}
