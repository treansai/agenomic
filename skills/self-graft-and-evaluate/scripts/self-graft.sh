#!/usr/bin/env bash
# Idempotent wrapper: detect, scaffold, validate.
# Usage: self-graft.sh [workspace] [--agent-id ID] [--name NAME]
set -euo pipefail

WORKSPACE="${1:-.}"
shift || true

AGENT_ID=""
AGENT_NAME=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --agent-id) AGENT_ID="$2"; shift 2 ;;
    --name)     AGENT_NAME="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 64 ;;
  esac
done

cd "$WORKSPACE"

command -v agenomic >/dev/null || {
  echo '{"status":"error","reason":"cli_missing"}'; exit 127;
}

# 1.1 Detect (dry-run)
agenomic init --dry-run --format json > .agenomic-detect.json

# 1.2 Scaffold (or update if bundle exists)
if [[ -d agent-bundle ]]; then
  agenomic update --no-commit
else
  args=(init)
  [[ -n "$AGENT_ID"   ]] && args+=(--agent-id "$AGENT_ID")
  [[ -n "$AGENT_NAME" ]] && args+=(--name "$AGENT_NAME")
  agenomic "${args[@]}"
fi

# 1.4 Validate strict
agenomic validate --level strict --format json > .agenomic-validate.json

echo '{"status":"pass","phase":"graft","bundle_path":"agent-bundle"}'
