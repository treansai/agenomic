#!/usr/bin/env bash
# Canonical copy: agenomic-cloud/scripts/ci/check-action-pins.sh (keep in sync).
# Every third-party action used by the workflows listed as arguments (default:
# all workflows under .github/workflows) must be pinned to a full commit SHA
# with a trailing comment naming the version (or alias) that SHA came from, e.g.
#   uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0
# Local composite actions (./...) and reusable workflows of this organisation
# referenced by SHA are accepted. Anything pinned to a tag or branch fails.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
if [ $# -gt 0 ]; then files=("$@"); else mapfile -t files < <(find "$root/.github/workflows" -name '*.yml' -o -name '*.yaml'); fi

status=0
for file in "${files[@]}"; do
  while IFS= read -r line; do
    ref="$(printf '%s' "$line" | sed -E 's/^[[:space:]-]*uses:[[:space:]]*//; s/[[:space:]]+#.*$//; s/["'"'"']//g')"
    case "$ref" in
      ./*) continue ;;
    esac
    target="${ref#*@}"
    if [[ ! "$target" =~ ^[0-9a-f]{40}$ ]]; then
      echo "check-action-pins: ${file#"$root"/}: '$ref' is not pinned to a commit SHA" >&2
      status=1
    elif ! printf '%s' "$line" | grep -Eq '#[[:space:]]*[^[:space:]]'; then
      echo "check-action-pins: ${file#"$root"/}: '$ref' lacks the '# <version>' comment" >&2
      status=1
    fi
  done < <(grep -E '^[[:space:]-]*uses:' "$file" || true)
done
[ "$status" -eq 0 ] && echo "check-action-pins: ok"
exit "$status"
