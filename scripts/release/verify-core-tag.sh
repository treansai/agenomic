#!/usr/bin/env bash
# Validates a Core release tag before anything is published:
#   - name core-vMAJOR.MINOR.PATCH[-pre]
#   - the tagged commit is on main (a release never comes from a branch)
#   - the tag does not move an existing release: the version is not already
#     published (immutability), and the crate versions match the tag
# Writes `version=` to --output when given (GITHUB_OUTPUT).
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tag=""; output=""
while [ $# -gt 0 ]; do
  case "$1" in
    --tag) tag="$2"; shift 2 ;;
    --output) output="$2"; shift 2 ;;
    *) echo "verify-core-tag: unknown argument $1" >&2; exit 2 ;;
  esac
done
fail() { echo "verify-core-tag: $*" >&2; exit 1; }
[ -n "$tag" ] || fail "--tag is required"
[[ "$tag" =~ ^core-v([0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?)$ ]] || fail "tag must be core-vMAJOR.MINOR.PATCH[-pre], got '$tag'"
version="${BASH_REMATCH[1]}"

sha="$(git -C "$root" rev-list -n 1 "$tag" 2>/dev/null || true)"
[ -n "$sha" ] || fail "tag $tag not found locally (fetch-depth: 0 required)"
git -C "$root" fetch -q origin main 2>/dev/null || true
git -C "$root" merge-base --is-ancestor "$sha" origin/main 2>/dev/null \
  || git -C "$root" merge-base --is-ancestor "$sha" main 2>/dev/null \
  || fail "tagged commit ${sha:0:12} is not on main; releases are cut from main only"

for manifest in "$root"/crates/*/Cargo.toml; do
  crate_version="$(grep -E '^version = "' "$manifest" | head -1 | sed -E 's/version = "(.*)"/\1/')"
  [ "$crate_version" = "${version%%-*}" ] || fail "$(basename "$(dirname "$manifest")") is at $crate_version, tag says ${version%%-*}; bump the crate versions first (single source of truth)"
done

if [ -n "${GITHUB_REPOSITORY:-}" ]; then
  if curl -fsS -o /dev/null -H "Accept: application/vnd.github+json" ${GH_TOKEN:+-H "Authorization: Bearer $GH_TOKEN"} \
       "https://api.github.com/repos/${GITHUB_REPOSITORY}/releases/tags/${tag}"; then
    fail "release $tag already exists; published releases are immutable"
  fi
fi

echo "verify-core-tag: $tag -> ${sha:0:12} ok"
[ -n "$output" ] && echo "version=$version" >> "$output"
exit 0
