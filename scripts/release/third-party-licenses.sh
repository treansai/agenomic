#!/usr/bin/env bash
# Third-party licence report for the shared crates, from `cargo metadata`
# (no extra tooling). --check fails when a dependency has no declared licence
# or declares one outside the redistributable allow-list; --output writes the
# Markdown report shipped with each Core release.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mode="report"; output=""
while [ $# -gt 0 ]; do
  case "$1" in
    --check) mode="check"; shift ;;
    --output) output="$2"; shift 2 ;;
    *) echo "third-party-licenses: unknown argument $1" >&2; exit 2 ;;
  esac
done
command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }
allowed='^(MIT|Apache-2\.0|BSD-2-Clause|BSD-3-Clause|ISC|Zlib|Unicode-3\.0|Unicode-DFS-2016|MPL-2\.0|CC0-1\.0|BSL-1\.0|0BSD|OpenSSL|Unlicense)$'

rows="$(cd "$root" && cargo metadata --format-version 1 --locked 2>/dev/null | jq -r '
  .packages[] | select(.source != null) | [.name, .version, (.license // "UNDECLARED"), (.repository // "")] | @tsv' | sort -u)"

status=0
while IFS=$'\t' read -r name version license _; do
  [ -n "$name" ] || continue
  ok=0
  IFS='/' read -r -a alts <<<"$(printf '%s' "$license" | sed -E 's/[[:space:]]*(OR|or)[[:space:]]*/\//g; s/[()]//g')"
  for alt in "${alts[@]}"; do
    alt="$(printf '%s' "$alt" | sed -E 's/^[[:space:]]+|[[:space:]]+$//g; s/ WITH .*$//')"
    [[ "$alt" =~ $allowed ]] && ok=1
  done
  if [ "$ok" -eq 0 ]; then
    echo "third-party-licenses: $name $version declares '$license' (not in the redistributable allow-list)" >&2
    status=1
  fi
done <<<"$rows"

if [ -n "$output" ]; then
  {
    echo "# Third-party licences"
    echo
    echo "Dependencies of the Agenomic Core crates, from Cargo.lock."
    echo
    echo "| Crate | Version | Licence | Repository |"
    echo "|---|---|---|---|"
    printf '%s\n' "$rows" | awk -F'\t' '{printf "| %s | %s | %s | %s |\n", $1, $2, $3, $4}'
  } > "$output"
  echo "third-party-licenses: report written to $output"
fi
if [ "$mode" = "check" ]; then
  [ "$status" -eq 0 ] && echo "third-party-licenses: ok"
  exit "$status"
fi
exit 0
