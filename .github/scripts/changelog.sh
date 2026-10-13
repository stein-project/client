#!/usr/bin/env bash
#
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# Copyright (c) 2026 Breno Xavier
#
# Release notes written by hand before a release, one file per language:
#
#   changelog/<version>.en_US.md    the public release text (English)
#   changelog/<version>.pt_BR.md    optional: the Portuguese text, kept with the project
#
# Every file has the same shape:
#
#   # Short title, one line
#
#   One summary paragraph.
#
#   - One item per change a player notices.
#
#   changelog.sh check <dir> <version>        the English notes present; both (the Portuguese ones when
#                                             written) with title and summary, no TODO
#   changelog.sh title|summary|body <file>
#
# The same file is kept in the installer, client and sdk repositories (.github/scripts/changelog.sh): change all
# three together.
set -euo pipefail

# CR stripped first: the Windows runner checks files out with CRLF.
title_of()   { tr -d '\r' < "$1" | sed -n '/^# /{s/^# //p;q;}'; }
# Summary: the first paragraph after the title that is not a list.
summary_of() { tr -d '\r' < "$1" | awk 'NR==1{next} /^#/{next} /^[[:space:]]*-/{exit} NF{print; got=1; next} got{exit}'; }
# Details: from the first list line on.
body_of()    { tr -d '\r' < "$1" | awk '/^[[:space:]]*-/{p=1} p'; }

cmd="${1:-}"; shift || true
case "$cmd" in
  check)
    dir="${1:?changelog dir}"; ver="${2:?version}"; fail=0
    for lang in en_US pt_BR; do
      f="$dir/$ver.$lang.md"
      [ -f "$f" ] || [ "$lang" != pt_BR ] || continue
      if [ ! -f "$f" ]; then echo "::error::missing $f: write what changed before releasing" >&2; fail=1; continue; fi
      [ -n "$(title_of "$f")" ]   || { echo "::error::$f: no title (the first line is '# ...')" >&2; fail=1; }
      [ -n "$(summary_of "$f")" ] || { echo "::error::$f: no summary paragraph" >&2; fail=1; }
      ! grep -q 'TODO' "$f"       || { echo "::error::$f: still has TODO" >&2; fail=1; }
    done
    [ "$fail" = 0 ] || exit 1
    echo "changelog ok:" "$dir/$ver".*.md
    ;;
  title)   title_of "${1:?file}" ;;
  summary) summary_of "${1:?file}" ;;
  body)    body_of "${1:?file}" ;;
  *) echo "usage: changelog.sh check <dir> <version> | title|summary|body <file>" >&2; exit 2 ;;
esac
