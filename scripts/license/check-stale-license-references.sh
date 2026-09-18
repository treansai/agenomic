#!/usr/bin/env bash
# Stale licence declarations in the documentation of the public tree.
#
# Only licence *declarations* are inspected: a shields.io licence badge, a
# `- License: X` metadata line, and the SPDX identifiers of the leading
# paragraph of a `## License` section. Each is compared with the licence of the component that owns the
# file, from packages.tsv. Naming a licence anywhere else is legitimate (a
# dependency report, a comparison, a third-party notice) and is not a finding,
# so the third-party material below is excluded outright.
# Usage: scripts/license/check-stale-license-references.sh
set -euo pipefail
# shellcheck source=scripts/license/lib.sh
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

root="$(license_root)"
status=0

# Third-party licence material: dependency reports, SBOMs, lockfiles, the
# licence texts themselves and the allow-list of the dependency scanner.
excluded_re='(^|/)(LICENSE|LICENCE|COPYING|NOTICE)([.-][^/]*)?$|(^|/)THIRD_PARTY[^/]*$|(^|/)(Cargo\.lock|package-lock\.json|pnpm-lock\.yaml|uv\.lock|poetry\.lock)$|\.spdx\.json$|(^|/)node_modules/|(^|/)scripts/release/third-party-licenses\.sh$|(^|/)scripts/license/'

# owner_license <path>: the licence of the component owning a file, from the
# longest classified prefix. Files outside every component belong to this
# repository, which is AGPL-3.0-only.
owner_license() {
  local file="$1" best="" best_len=0 dir len
  while IFS=$'\t' read -r path edition licence _rest; do
    [ "$edition" = "oss" ] || continue
    dir="$(dirname "$path")"
    [ "$dir" = "." ] && continue
    case "$file" in "$dir"/*) len=${#dir}; if [ "$len" -gt "$best_len" ]; then best="$licence"; best_len=$len; fi ;; esac
  done < <(rows "$root")
  [ -n "$best" ] && { printf '%s' "$best"; return; }
  printf 'AGPL-3.0-only'
}

report() { echo "stale-license: $1:$2 declares '$3', expected '$4'" >&2; status=1; }

while IFS= read -r file; do
  case "$file" in *.md|*.markdown) ;; *) continue ;; esac
  [[ "$file" =~ $excluded_re ]] && continue
  [ -f "$root/$file" ] || continue
  expected="$(owner_license "$file")"

  while IFS=: read -r line found; do
    [ -n "$found" ] || continue
    [ "$found" = "$expected" ] || report "$file" "$line" "$found" "$expected"
  done < <(
    grep -nE 'img\.shields\.io/badge/license-[^)]+' "$root/$file" |
      sed -E 's/^([0-9]+):.*img\.shields\.io\/badge\/license-([^-)]*(--[^-)]*)*)-[a-z]+\.svg.*/\1:\2/; s/--/-/g' || true
    grep -nE '^- License: .+$' "$root/$file" | sed -E 's/^([0-9]+):- License: (.*)$/\1:\2/' || true
    awk '/^## Licen[cs]e[[:space:]]*$/ { para = 1; next }
         para == 1 && !NF { next }
         para == 2 && !NF { para = 0 }
         para { para = 2
           line = $0
           while (match(line, /AGPL-3\.0-(only|or-later)|Apache-2\.0|BSD-[23]-Clause|MPL-2\.0|GPL-3\.0-(only|or-later)|MIT|ISC/)) {
             print NR ":" substr(line, RSTART, RLENGTH)
             line = substr(line, RSTART + RLENGTH)
           }
         }' "$root/$file" || true
  )
done < <(git -C "$root" ls-files --recurse-submodules 2>/dev/null || git -C "$root" ls-files)

if [ "$status" -eq 0 ]; then echo "check-stale-license-references: ok"; fi
exit "$status"
