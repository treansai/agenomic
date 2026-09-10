#!/usr/bin/env bash
# Every submodule of the public umbrella must point at a commit that exists on
# its public origin. A pointer to a private fork, an unpushed local commit or
# a rewritten history fails the release.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
status=0
while read -r path; do
  [ -n "$path" ] || continue
  url="$(git -C "$root" config -f .gitmodules "submodule.${path}.url")"
  sha="$(git -C "$root" ls-tree HEAD "$path" | awk '{print $3}')"
  [ -n "$sha" ] || { echo "check-submodule-pointers: $path has no gitlink in HEAD" >&2; status=1; continue; }
  case "$url" in
    https://github.com/treansai/*) ;;
    *) echo "check-submodule-pointers: $path points at a non-public origin: $url" >&2; status=1; continue ;;
  esac
  repo="${url#https://github.com/}"; repo="${repo%.git}"
  if ! curl -fsS -o /dev/null -H "Accept: application/vnd.github+json" ${GH_TOKEN:+-H "Authorization: Bearer $GH_TOKEN"} "https://api.github.com/repos/${repo}/commits/${sha}"; then
    echo "check-submodule-pointers: $path -> ${repo}@${sha:0:12} is not reachable on GitHub" >&2; status=1
  else
    echo "check-submodule-pointers: $path -> ${repo}@${sha:0:12} ok"
  fi
done < <(git -C "$root" config -f .gitmodules --get-regexp '^submodule\..*\.path$' | awk '{print $2}')
exit "$status"
