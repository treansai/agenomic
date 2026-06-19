#!/usr/bin/env bash
# Idempotent wrapper: detect, scaffold, validate (phases 1.1 -> 1.4).
# Spec-version aware: handles agent (genome), workflow, and system bundles.
# Usage: self-graft.sh [workspace] [--bundle-dir DIR] [--agent-id ID] [--name NAME]
set -euo pipefail

WORKSPACE="${1:-.}"
shift || true

BUNDLE_DIR="."
AGENT_ID=""
AGENT_NAME=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --bundle-dir) BUNDLE_DIR="$2"; shift 2 ;;
    --agent-id)   AGENT_ID="$2"; shift 2 ;;
    --name)       AGENT_NAME="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 64 ;;
  esac
done

cd "$WORKSPACE"

# Prefer the `agm` alias; fall back to `agenomic`.
CLI=""
for c in agm agenomic; do
  if command -v "$c" >/dev/null 2>&1; then CLI="$c"; break; fi
done
[[ -n "$CLI" ]] || { echo '{"status":"error","reason":"cli_missing"}'; exit 127; }

# 1.1 Detect (dry-run; also reports v0.2 orchestration: workflows, system, env)
"$CLI" init "$BUNDLE_DIR" --dry-run --format json > .agenomic-detect.json

# 1.2 Scaffold (init), or merge if a bundle already exists (update).
if [[ -f "$BUNDLE_DIR/genome.yaml" || -f "$BUNDLE_DIR/system.yaml" ]]; then
  "$CLI" update "$BUNDLE_DIR" --no-commit
else
  args=(init "$BUNDLE_DIR")
  [[ -n "$AGENT_ID"   ]] && args+=(--agent-id "$AGENT_ID")
  [[ -n "$AGENT_NAME" ]] && args+=(--name "$AGENT_NAME")
  "$CLI" "${args[@]}"
fi

# Determine the bundle shape for the report.
if [[ -f "$BUNDLE_DIR/system.yaml" && ! -f "$BUNDLE_DIR/genome.yaml" ]]; then
  KIND="system"
elif [[ -d "$BUNDLE_DIR/workflows" || -f "$BUNDLE_DIR/workflow.yaml" ]]; then
  KIND="workflow"
else
  KIND="agent"
fi

# 1.4 Validate the whole bundle (genome/lock/contract + workflows/*.yaml + system.yaml).
"$CLI" validate "$BUNDLE_DIR" --level strict --format json > .agenomic-validate.json

printf '{"status":"pass","phase":"graft","kind":"%s","bundle_path":"%s"}\n' \
  "$KIND" "$BUNDLE_DIR"
