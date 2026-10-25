#!/bin/sh
# Tauri requires SemVer (three numeric components). The game may use a fourth numeric
# component, which is retained as SemVer build metadata; pull-request suffixes are omitted.
set -eu
version=${1%%-*}
old_ifs=$IFS
IFS=.
set -- $version
IFS=$old_ifs

if [ "$#" -ne 3 ] && [ "$#" -ne 4 ]; then
  echo "Tauri version must have three or four numeric components: $version" >&2
  exit 1
fi
for part in "$@"; do
  case "$part" in
    ''|*[!0-9]*)
      echo "Tauri version must have three or four numeric components: $version" >&2
      exit 1
      ;;
  esac
done

tauri_version="$1.$2.$3"
if [ "$#" -eq 4 ]; then
  tauri_version="$tauri_version+$4"
fi
echo "$tauri_version"
