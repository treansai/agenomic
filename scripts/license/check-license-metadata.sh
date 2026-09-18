#!/usr/bin/env bash
# Licence metadata of the public tree, against scripts/license/packages.tsv.
#
# Fails when the root LICENSE is missing or is not the GNU AGPL v3 text, when
# a package declares a licence other than the one it is classified under, when
# a PyPI classifier contradicts that licence, or when a package that must ship
# a LICENSE file has none. A submodule that is not checked out is reported and
# skipped, so the check is usable before `git submodule update --init`.
# Usage: scripts/license/check-license-metadata.sh
set -euo pipefail
# shellcheck source=scripts/license/lib.sh
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

root="$(license_root)"
status=0
skipped=0

if [ ! -f "$root/LICENSE" ]; then
  echo "license-metadata: root LICENSE is missing" >&2
  status=1
elif ! is_agpl_text "$root/LICENSE"; then
  echo "license-metadata: root LICENSE is not the GNU Affero General Public License v3 text" >&2
  status=1
fi

while IFS=$'\t' read -r path edition licence _published licence_file; do
  [ "$edition" = "oss" ] || continue
  manifest="$root/$path"
  if [ ! -f "$manifest" ]; then
    echo "license-metadata: $path not checked out, skipped"
    skipped=$((skipped + 1))
    continue
  fi

  declared="$(declared_license "$manifest")"
  if [ -z "$declared" ]; then
    echo "license-metadata: $path declares no licence (expected $licence)" >&2
    status=1
  elif [ "$declared" != "$licence" ]; then
    echo "license-metadata: $path declares '$declared', expected '$licence'" >&2
    status=1
  fi

  if [ "$(basename "$path")" = "pyproject.toml" ]; then
    found="$(grep -E '^[[:space:]]*"License :: ' "$manifest" || true)"
    if [ -n "$found" ]; then
      expected="$(classifier_for "$licence")"
      if [ -z "$expected" ] || ! printf '%s' "$found" | grep -qF "$expected"; then
        echo "license-metadata: $path has a trove classifier that contradicts $licence:" >&2
        printf '  %s\n' "$found" >&2
        status=1
      fi
    fi
  fi

  if [ "$licence_file" = "yes" ] && [ ! -f "$(dirname "$manifest")/LICENSE" ]; then
    echo "license-metadata: $path has no LICENSE file next to it" >&2
    status=1
  fi
done < <(rows "$root")

if [ "$status" -eq 0 ]; then
  echo "check-license-metadata: ok ($skipped skipped)"
fi
exit "$status"
