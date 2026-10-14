#!/bin/sh
# Print the version of the checked-out commit: MAJOR.MINOR.COMMIT.
#   MAJOR.MINOR  the VERSION file (edited by hand)
#   COMMIT       the commits since VERSION last changed (0 on the commit that changes it)
# See docs/VERSIONING.md. Needs the full history (CI: fetch-depth: 0).
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
base=$(tr -d ' \r\n' < VERSION)
last=$(git log -1 --format=%H -- VERSION 2>/dev/null || true)
if [ -n "$last" ]; then n=$(git rev-list --count "$last..HEAD"); else n=0; fi
echo "$base.$n"
