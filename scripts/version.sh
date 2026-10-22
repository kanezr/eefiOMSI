#!/bin/sh
# Print the version of the checked-out commit: MAJOR.MINOR.PATCH.
#   MAJOR.MINOR  the VERSION file (edited by hand)
#   PATCH        the release tagged on this commit, or one more than the last release
#                v<MAJOR.MINOR>.* (0 when there is none yet)
# See docs/VERSIONING.md. Needs the tags (CI: fetch-depth: 0).
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
base=$(tr -d ' \r\n' < VERSION)
last() { git tag -l "$@" "v$base.*" 2>/dev/null | sed "s/^v$base\.//" | grep -E '^[0-9]+$' | sort -n | tail -1; }
n=$(last --points-at HEAD)
if [ -z "$n" ]; then
  n=$(last)
  if [ -n "$n" ]; then n=$((n + 1)); else n=0; fi
fi
echo "$base.$n"
