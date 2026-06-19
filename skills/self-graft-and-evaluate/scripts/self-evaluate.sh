#!/usr/bin/env bash
# Idempotent wrapper: replay, contract checks, diff vs. previous bundle, report
# (phases 2.1 -> 3). Spec-version aware: agent / workflow / system bundles.
# Usage: self-evaluate.sh [workspace] [--bundle-dir DIR] [--against REF]
set -euo pipefail

WORKSPACE="${1:-.}"
shift || true

BUNDLE_DIR="."
AGAINST=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --bundle-dir) BUNDLE_DIR="$2"; shift 2 ;;
    --against)    AGAINST="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 64 ;;
  esac
done

cd "$WORKSPACE"

CLI=""
for c in agm agenomic; do
  if command -v "$c" >/dev/null 2>&1; then CLI="$c"; break; fi
done
[[ -n "$CLI" ]] || { echo '{"status":"error","reason":"cli_missing"}'; exit 127; }

[[ -f "$BUNDLE_DIR/genome.yaml" || -f "$BUNDLE_DIR/system.yaml" ]] || {
  echo '{"status":"error","reason":"no_bundle"}'; exit 65;
}

# Bundle shape + primary spec_version (best-effort).
if [[ -f "$BUNDLE_DIR/system.yaml" && ! -f "$BUNDLE_DIR/genome.yaml" ]]; then
  KIND="system"; PRIMARY="$BUNDLE_DIR/system.yaml"
elif [[ -d "$BUNDLE_DIR/workflows" || -f "$BUNDLE_DIR/workflow.yaml" ]]; then
  KIND="workflow"; PRIMARY="$BUNDLE_DIR/genome.yaml"
else
  KIND="agent"; PRIMARY="$BUNDLE_DIR/genome.yaml"
fi
SPEC_VERSION="$(grep -m1 -E '^[[:space:]]*spec_version:' "$PRIMARY" 2>/dev/null \
  | sed -E "s/.*spec_version:[[:space:]]*['\"]?([^'\"]+)['\"]?.*/\1/" || true)"
SPEC_VERSION="${SPEC_VERSION:-unknown}"

# 2.1 Replay — only when there is a genome and recorded traces to re-execute.
REPLAY_JSON="null"
if [[ -f "$BUNDLE_DIR/genome.yaml" && -d "$BUNDLE_DIR/evals" ]]; then
  replay_args=(replay "$BUNDLE_DIR" "$BUNDLE_DIR/evals")
  [[ -f "$BUNDLE_DIR/behavior.contract.yaml" ]] && \
    replay_args+=(--contract "$BUNDLE_DIR/behavior.contract.yaml")
  "$CLI" "${replay_args[@]}" --format json > .agenomic-replay.json || true
  REPLAY_JSON="$(cat .agenomic-replay.json 2>/dev/null || echo null)"
fi

# 2.3 Diff vs. a previous bundle (optional). Surfaces capability/tool/policy/
# model changes and, for v0.2, workflow/orchestration/membership changes.
DIFF_JSON="null"
if [[ -n "$AGAINST" ]]; then
  "$CLI" diff "$AGAINST" "$BUNDLE_DIR" --format json > .agenomic-diff.json || true
  DIFF_JSON="$(cat .agenomic-diff.json 2>/dev/null || echo null)"
fi

# 3 Report
cat > "$BUNDLE_DIR/.self-evaluation.json" <<EOF
{
  "skill": "self-graft-and-evaluate",
  "skill_version": "0.2.0",
  "kind": "${KIND}",
  "spec_version": "${SPEC_VERSION}",
  "phases": {
    "graft":    { "status": "pass", "bundle_path": "${BUNDLE_DIR}" },
    "evaluate": { "replay": ${REPLAY_JSON}, "diff": ${DIFF_JSON} }
  }
}
EOF

echo "self-evaluation (kind: ${KIND}, spec_version: ${SPEC_VERSION}) written to ${BUNDLE_DIR}/.self-evaluation.json"
