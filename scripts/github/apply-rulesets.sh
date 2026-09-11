#!/usr/bin/env bash
# Apply (or preview) the repository rulesets that make `main` and the
# `core-v*` release tags immutable and review-gated.
#
#   scripts/github/apply-rulesets.sh --repo treansai/agenomic [--dry-run]
#
# Rulesets are available on public repositories on every GitHub plan; on
# private repositories they require GitHub Team or higher (the agenomic-cloud,
# agenomic-web and agenomic-infra repositories are on the Free plan today, so
# this script refuses them unless --force is given, and the API will answer
# 403 until the plan changes). Idempotent: an existing ruleset with the same
# name is updated in place. `creation` on the tag ruleset means only the
# repository admin bypass (actor_id 5) can create a core-v* tag, i.e. the
# release is cut deliberately by a maintainer, never by a bot.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo=""; dry_run=0; force=0
while [ $# -gt 0 ]; do
  case "$1" in
    --repo) repo="$2"; shift 2 ;;
    --dry-run) dry_run=1; shift ;;
    --force) force=1; shift ;;
    *) echo "apply-rulesets: unknown argument $1" >&2; exit 2 ;;
  esac
done
[ -n "$repo" ] || { echo "apply-rulesets: --repo owner/name is required" >&2; exit 2; }
command -v gh >/dev/null || { echo "gh is required" >&2; exit 1; }
command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }

visibility="$(gh repo view "$repo" --json visibility -q .visibility)"
if [ "$visibility" != "PUBLIC" ] && [ "$force" -ne 1 ]; then
  echo "apply-rulesets: $repo is $visibility; rulesets need GitHub Team or higher on private repositories (--force to try anyway)" >&2
  exit 1
fi

for file in "$here"/rulesets/*.json; do
  name="$(jq -r .name "$file")"
  existing="$(gh api "repos/${repo}/rulesets" --jq ".[] | select(.name == \"${name}\") | .id" 2>/dev/null || true)"
  if [ "$dry_run" -eq 1 ]; then
    echo "apply-rulesets: would $([ -n "$existing" ] && echo "update #$existing" || echo create) '$name' on $repo from $(basename "$file")"
    continue
  fi
  if [ -n "$existing" ]; then
    gh api -X PUT "repos/${repo}/rulesets/${existing}" --input "$file" >/dev/null
    echo "apply-rulesets: updated '$name' (#$existing) on $repo"
  else
    gh api -X POST "repos/${repo}/rulesets" --input "$file" >/dev/null
    echo "apply-rulesets: created '$name' on $repo"
  fi
done
