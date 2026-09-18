#!/usr/bin/env bash
# OSS / Cloud boundary of the licence classification.
#
# Every package of the public tree must be classified in packages.tsv, so a new
# package cannot be published with an unreviewed licence; and no component
# classified `cloud` may have a manifest here, so a private component is never
# reclassified as OSS by a synchronisation. Manifests that inherit their
# licence from a workspace, and virtual workspace manifests, are covered by the
# workspace row and are not classified again, but each of them must still ship
# the licence text, because cargo packages a crate directory and nothing above
# it.
# Usage: scripts/license/check-public-packages.sh
set -euo pipefail
# shellcheck source=scripts/license/lib.sh
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

root="$(license_root)"
status=0

tracked() {
  git -C "$root" ls-files --recurse-submodules 2>/dev/null || git -C "$root" ls-files
}

classified="$(rows "$root" | cut -f1)"
cloud="$(rows "$root" | awk -F'\t' '$2 == "cloud" { print $1 }')"

while IFS= read -r file; do
  case "$(basename "$file")" in
    package.json|pyproject.toml|Cargo.toml) ;;
    *) continue ;;
  esac
  case "$file" in */node_modules/*) continue ;; esac
  if [ "$(basename "$file")" = "Cargo.toml" ]; then
    grep -q '^\[package\]' "$root/$file" || continue
    if ! grep -q '^publish[[:space:]]*=[[:space:]]*false' "$root/$file" &&
       [ ! -f "$(dirname "$root/$file")/LICENSE" ]; then
      echo "public-packages: $file is published but ships no LICENSE file" >&2
      status=1
    fi
    grep -q '^license\.workspace[[:space:]]*=[[:space:]]*true' "$root/$file" && continue
  fi
  printf '%s\n' "$classified" | grep -qxF "$file" && continue
  echo "public-packages: $file is not classified in scripts/license/packages.tsv" >&2
  status=1
done < <(tracked)

while IFS= read -r name; do
  [ -n "$name" ] || continue
  hits="$(tracked | grep -E "(^|/)${name}/" || true)"
  if [ -n "$hits" ]; then
    echo "public-packages: cloud component '$name' has files in the public tree:" >&2
    printf '%s\n' "$hits" | head -10 >&2
    status=1
  fi
done <<EOT
$cloud
EOT

if [ "$status" -eq 0 ]; then echo "check-public-packages: ok"; fi
exit "$status"
